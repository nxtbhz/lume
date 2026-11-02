use crate::app::{ActiveFocus, App};
use crate::split::HitTarget;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use lume_core::TextBuffer;
use std::time::{Duration, Instant};

impl App {
    pub fn handle_mouse(&mut self, mouse: MouseEvent) -> std::io::Result<()> {
        let col = mouse.column as usize;
        let row = mouse.row as usize;

        if let Some(mut state) = self.quick_open.take() {
            let term_w = self.terminal_width as usize;
            let term_h = self.terminal_height as usize;
            let box_w = (term_w * 3 / 4).clamp(50, 84).min(term_w);
            let box_h = 14.min(term_h.saturating_sub(4)).max(6);
            let start_x = term_w.saturating_sub(box_w) / 2;
            let start_y = 3.min(term_h.saturating_sub(box_h) / 2);

            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if col >= start_x
                        && col < start_x + box_w
                        && row >= start_y
                        && row < start_y + box_h
                    {
                        if row >= start_y + 3 {
                            let item_idx = state.scroll_offset + (row - (start_y + 3));
                            if item_idx < state.filtered.len() {
                                state.selected_idx = item_idx;
                                if let Some(item) = state.selected_item() {
                                    let full_path = item.full_path.clone();
                                    self.quick_open = None;
                                    match self.editor.open_or_load(&full_path) {
                                        Ok(_) => {
                                            self.focus = ActiveFocus::Editor;
                                            self.adjust_viewport();
                                            self.set_status(format!(
                                                "Opened {}",
                                                full_path.display()
                                            ));
                                        }
                                        Err(e) => self.set_status(format!("Open error: {}", e)),
                                    }
                                    return Ok(());
                                }
                            }
                        }
                        self.quick_open = Some(state);
                    } else {
                        // Click outside closes quick open
                        self.quick_open = None;
                    }
                    return Ok(());
                }
                MouseEventKind::ScrollUp => {
                    state.move_up();
                    self.quick_open = Some(state);
                    return Ok(());
                }
                MouseEventKind::ScrollDown => {
                    state.move_down(10);
                    self.quick_open = Some(state);
                    return Ok(());
                }
                _ => {
                    self.quick_open = Some(state);
                    return Ok(());
                }
            }
        }

        let exp_w = self.explorer_width();
        let gutter_w = self.gutter_width();
        let status_row = self.terminal_height.saturating_sub(1) as usize;
        let text_left = self.text_area_left();
        let text_top = self.text_area_top();
        let area_w = (self.terminal_width as usize).saturating_sub(text_left);
        let text_h = self.text_area_height();

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.pending_chord = None;
                // Click in status bar (Row status_row)
                if row == status_row {
                    // Click on language area (Ln ..., Col ... | <Lang>)
                    self.execute_command("language.set_mode");
                    return Ok(());
                }

                // Click in Tab Bar (Row 0)
                if row == 0 {
                    if col < exp_w {
                        // Click in explorer header: toggle focus or refresh
                        self.focus = ActiveFocus::Explorer;
                        self.explorer.refresh();
                    } else {
                        self.focus = ActiveFocus::Editor;
                        self.handle_tab_bar_click(col.saturating_sub(exp_w));
                    }
                    return Ok(());
                }

                // Click in main body (Rows 1..status_row)
                if row >= text_top && row < status_row {
                    if col < exp_w && self.explorer.is_visible {
                        // Click in File Explorer
                        self.focus = ActiveFocus::Explorer;
                        let rel_row = row - text_top;
                        if let Some(path) = self.explorer.click_at(rel_row) {
                            match self.editor.open_or_load(&path) {
                                Ok(_) => {
                                    self.focus = ActiveFocus::Editor;
                                    self.sync_active_pane();
                                    self.set_status(format!("Opened {}", path.display()));
                                }
                                Err(e) => self.set_status(format!("Open error: {}", e)),
                            }
                        }
                        return Ok(());
                    }

                    // Click in Editor text area
                    self.search_state = None;
                    self.focus = ActiveFocus::Editor;

                    let (buffer_line, buffer_col) = if self.splits.is_split {
                        match self
                            .splits
                            .hit_test(col, row, text_left, text_top, area_w, text_h)
                        {
                            HitTarget::Pane(target_pane) => {
                                if target_pane != self.splits.active_pane {
                                    self.switch_active_pane(target_pane);
                                }
                                let (p0_rect, _, p1_rect) =
                                    self.splits.layout(text_left, text_top, area_w, text_h);
                                let pane_rect = if target_pane == 0 {
                                    p0_rect
                                } else {
                                    p1_rect.unwrap_or(p0_rect)
                                };
                                let pane_gutter_w = self.gutter_width().min(pane_rect.width);
                                let b_line = self.scroll_row + row.saturating_sub(pane_rect.top);
                                let b_col = self.scroll_col.saturating_add(
                                    col.saturating_sub(pane_rect.left + pane_gutter_w),
                                );
                                (b_line, b_col)
                            }
                            HitTarget::Separator | HitTarget::Outside => {
                                self.mouse_dragging = false;
                                return Ok(());
                            }
                        }
                    } else {
                        let b_line = self.scroll_row + (row - text_top);
                        let b_col = self
                            .scroll_col
                            .saturating_add(col.saturating_sub(text_left + gutter_w));
                        (b_line, b_col)
                    };

                    let now = Instant::now();
                    if now.duration_since(self.last_click_time) < Duration::from_millis(400)
                        && self.last_click_pos == (mouse.column, mouse.row)
                    {
                        self.click_count = self.click_count.saturating_add(1);
                    } else {
                        self.click_count = 1;
                    }
                    self.last_click_time = now;
                    self.last_click_pos = (mouse.column, mouse.row);

                    if self.click_count == 2 {
                        self.editor
                            .set_cursor_from_line_col(buffer_line, buffer_col, false);
                        self.editor.select_word_at_cursor();
                        self.mouse_dragging = false;
                    } else if self.click_count >= 3 {
                        self.editor
                            .set_cursor_from_line_col(buffer_line, buffer_col, false);
                        self.editor.select_line_at_cursor();
                        self.mouse_dragging = false;
                    } else {
                        self.editor
                            .set_cursor_from_line_col(buffer_line, buffer_col, false);
                        self.mouse_dragging = true;
                    }
                    self.sync_active_pane();
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.mouse_dragging && row >= text_top && row < status_row && col >= text_left {
                    let (buffer_line, buffer_col) = if self.splits.is_split {
                        let (p0_rect, _, p1_rect) =
                            self.splits.layout(text_left, text_top, area_w, text_h);
                        let pane_rect = if self.splits.active_pane == 0 {
                            p0_rect
                        } else {
                            p1_rect.unwrap_or(p0_rect)
                        };
                        let pane_gutter_w = self.gutter_width().min(pane_rect.width);
                        let rel_row = row
                            .saturating_sub(pane_rect.top)
                            .min(pane_rect.height.saturating_sub(1));
                        let b_line = self.scroll_row + rel_row;
                        let b_col = self
                            .scroll_col
                            .saturating_add(col.saturating_sub(pane_rect.left + pane_gutter_w));
                        (b_line, b_col)
                    } else {
                        let b_line = self.scroll_row + (row - text_top);
                        let b_col = self
                            .scroll_col
                            .saturating_add(col.saturating_sub(text_left + gutter_w));
                        (b_line, b_col)
                    };
                    self.editor
                        .set_cursor_from_line_col(buffer_line, buffer_col, true);
                    self.sync_active_pane();
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.mouse_dragging = false;
            }
            MouseEventKind::ScrollUp => {
                if col < exp_w && self.explorer.is_visible {
                    self.explorer.move_up();
                } else if self.splits.is_split {
                    if let HitTarget::Pane(target_pane) = self
                        .splits
                        .hit_test(col, row, text_left, text_top, area_w, text_h)
                    {
                        self.splits.panes[target_pane].scroll_row =
                            self.splits.panes[target_pane].scroll_row.saturating_sub(3);
                        if target_pane == self.splits.active_pane {
                            self.scroll_row = self.splits.panes[target_pane].scroll_row;
                        }
                    }
                } else {
                    self.scroll_row = self.scroll_row.saturating_sub(3);
                }
            }
            MouseEventKind::ScrollDown => {
                if col < exp_w && self.explorer.is_visible {
                    self.explorer.move_down(text_h);
                } else if self.splits.is_split {
                    if let HitTarget::Pane(target_pane) = self
                        .splits
                        .hit_test(col, row, text_left, text_top, area_w, text_h)
                    {
                        let tab_idx = self.splits.panes[target_pane]
                            .tab_idx
                            .min(self.editor.tabs().len().saturating_sub(1));
                        let total_lines = self.editor.tabs()[tab_idx].buffer.len_lines();
                        let max_lines = total_lines.saturating_sub(1);
                        self.splits.panes[target_pane].scroll_row =
                            (self.splits.panes[target_pane].scroll_row + 3).min(max_lines);
                        if target_pane == self.splits.active_pane {
                            self.scroll_row = self.splits.panes[target_pane].scroll_row;
                        }
                    }
                } else {
                    let total_lines = self.editor.buffer().len_lines();
                    let max_lines = total_lines.saturating_sub(1);
                    self.scroll_row = (self.scroll_row + 3).min(max_lines);
                }
            }
            MouseEventKind::ScrollLeft => {
                if self.splits.is_split {
                    if let HitTarget::Pane(target_pane) = self
                        .splits
                        .hit_test(col, row, text_left, text_top, area_w, text_h)
                    {
                        self.splits.panes[target_pane].scroll_col =
                            self.splits.panes[target_pane].scroll_col.saturating_sub(3);
                        if target_pane == self.splits.active_pane {
                            self.scroll_col = self.splits.panes[target_pane].scroll_col;
                        }
                    }
                } else {
                    self.scroll_col = self.scroll_col.saturating_sub(3);
                }
            }
            MouseEventKind::ScrollRight => {
                if self.splits.is_split {
                    if let HitTarget::Pane(target_pane) = self
                        .splits
                        .hit_test(col, row, text_left, text_top, area_w, text_h)
                    {
                        self.splits.panes[target_pane].scroll_col =
                            self.splits.panes[target_pane].scroll_col.saturating_add(3);
                        if target_pane == self.splits.active_pane {
                            self.scroll_col = self.splits.panes[target_pane].scroll_col;
                        }
                    }
                } else {
                    self.scroll_col = self.scroll_col.saturating_add(3);
                }
            }
            _ => {}
        }

        Ok(())
    }

    pub fn handle_tab_bar_click(&mut self, col: usize) {
        let mut current_col = 0;
        for (i, tab) in self.editor.tabs().iter().enumerate() {
            let dirty = if tab.is_dirty { "*" } else { "" };
            let is_active = i == self.editor.active_tab_idx();
            let tab_text = if is_active {
                format!(" [ {}: {}{} x ] ", i + 1, tab.name(), dirty)
            } else {
                format!("  {}: {}{} x   ", i + 1, tab.name(), dirty)
            };

            let tab_len = tab_text.chars().count();
            if col >= current_col && col < current_col + tab_len {
                // Check if clicked the 'x' button inside tab (always located 4 characters before the end)
                let close_button_offset = tab_len.saturating_sub(4);
                if col >= current_col + close_button_offset - 1
                    && col <= current_col + close_button_offset + 1
                {
                    self.close_tab_and_sync_splits(i);
                } else {
                    self.editor.switch_tab(i);
                }
                self.adjust_viewport();
                return;
            }
            current_col += tab_len;
        }

        // Check if clicked the '[ + ]' button
        if col >= current_col && col < current_col + 6 {
            self.editor.new_tab();
            self.adjust_viewport();
        }
    }
}
