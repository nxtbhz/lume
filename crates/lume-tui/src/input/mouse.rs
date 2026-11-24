use crate::app::{ActiveFocus, App};
use crate::quick_open::{self, HEADER_ROWS};
use crate::render::tab_bar::tab_label;
use crate::split::HitTarget;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use lume_core::buffer::{visual, LINE_BREAKS};
use lume_core::TextBuffer;
use std::time::{Duration, Instant};

impl App {
    pub fn handle_mouse(&mut self, mouse: MouseEvent) -> std::io::Result<bool> {
        let col = mouse.column as usize;
        let row = mouse.row as usize;

        if mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && mouse.modifiers.contains(KeyModifiers::CONTROL)
        {
            return Ok(self.handle_ctrl_left_click(col, row));
        }

        if let Some(mut state) = self.quick_open.take() {
            let rect =
                quick_open::layout(self.terminal_width as usize, self.terminal_height as usize);

            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if col >= rect.left
                        && col < rect.left + rect.width
                        && row >= rect.top
                        && row < rect.top + rect.height
                    {
                        let first_item_row = rect.top + HEADER_ROWS;
                        if row >= first_item_row {
                            let item_idx = state.visible_offset(rect.height) + row - first_item_row;
                            if item_idx < state.filtered.len() {
                                state.selected_idx = item_idx;
                                if let Some(item) = state.selected_item() {
                                    let full_path = item.full_path.clone();
                                    self.quick_open = None;
                                    self.open_file_into_editor(
                                        &full_path,
                                        "Opened ",
                                        "Open error: ",
                                    );
                                    return Ok(true);
                                }
                            }
                        }
                        self.quick_open = Some(state);
                        return Ok(false);
                    } else {
                        // Click outside closes quick open
                        self.quick_open = None;
                        return Ok(true);
                    }
                }
                MouseEventKind::ScrollUp => {
                    state.move_up();
                    self.quick_open = Some(state);
                    return Ok(true);
                }
                MouseEventKind::ScrollDown => {
                    state.move_down(rect.height.saturating_sub(HEADER_ROWS));
                    self.quick_open = Some(state);
                    return Ok(true);
                }
                _ => {
                    self.quick_open = Some(state);
                    return Ok(false);
                }
            }
        }

        let exp_w = self.explorer_width();
        let status_row = self.terminal_height.saturating_sub(1) as usize;
        let text_left = self.text_area_left();
        let text_top = self.text_area_top();
        let area_w = (self.terminal_width as usize).saturating_sub(text_left);
        let text_h = self.text_area_height();

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.clear_pending_keys();
                // Click in status bar (Row status_row)
                if row == status_row {
                    // Click on language area (Ln ..., Col ... | <Lang>)
                    self.execute_command("language.set_mode");
                    return Ok(true);
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
                    return Ok(true);
                }

                // Click in main body (Rows 1..status_row)
                if row >= text_top && row < status_row {
                    if col < exp_w && self.explorer.is_visible {
                        // Click in File Explorer
                        self.focus = ActiveFocus::Explorer;
                        let rel_row = row - text_top;
                        if let Some(path) = self.explorer.click_at(rel_row) {
                            self.open_file_into_editor(&path, "Opened ", "Open error: ");
                        }
                        return Ok(true);
                    }

                    // Click in Editor text area
                    self.search_state = None;
                    self.focus = ActiveFocus::Editor;

                    let HitTarget::Pane(pane) = self
                        .splits
                        .hit_test(col, row, text_left, text_top, area_w, text_h)
                    else {
                        self.mouse_dragging = false;
                        return Ok(false);
                    };
                    self.switch_active_pane(pane);
                    let (byte, _) = self.mouse_byte(pane, col, row);
                    self.editor.cursors_mut().primary_mut().move_to(byte);
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
                        self.editor.select_word_at_cursor();
                        self.mouse_dragging = false;
                    } else if self.click_count >= 3 {
                        self.editor.select_line_at_cursor();
                        self.mouse_dragging = false;
                    } else {
                        self.mouse_dragging = true;
                    }
                    self.sync_active_pane();
                    return Ok(true);
                }
                Ok(false)
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.mouse_dragging && row >= text_top && row < status_row && col >= text_left {
                    let (byte, _) = self.mouse_byte(self.splits.active_pane, col, row);
                    self.editor.cursors_mut().primary_mut().select_to(byte);
                    self.sync_active_pane();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.mouse_dragging = false;
                Ok(false)
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let down = mouse.kind == MouseEventKind::ScrollDown;
                if col < exp_w && self.explorer.is_visible {
                    if down {
                        self.explorer.move_down(text_h);
                    } else {
                        self.explorer.move_up();
                    }
                } else if let HitTarget::Pane(pane) = self
                    .splits
                    .hit_test(col, row, text_left, text_top, area_w, text_h)
                {
                    self.scroll_pane_visual_rows(Some(pane), if down { 3 } else { -3 });
                }
                Ok(true)
            }
            MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight if self.word_wrap => Ok(false),
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
                Ok(true)
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
                Ok(true)
            }
            MouseEventKind::Moved => Ok(false),
            _ => Ok(false),
        }
    }

    pub fn handle_tab_bar_click(&mut self, col: usize) {
        let mut current_col = 0;
        for (i, tab) in self.editor.tabs().iter().enumerate() {
            let label = tab_label(tab, i, i == self.editor.active_tab_idx());
            let tab_len: usize = visual::graphemes_from(&label, current_col)
                .map(|g| g.width)
                .sum();
            if col >= current_col && col < current_col + tab_len {
                // Check if clicked the 'x' button inside tab (always located 4 characters before the end)
                let close_button_offset = tab_len.saturating_sub(4);
                if col >= current_col + close_button_offset - 1
                    && col <= current_col + close_button_offset + 1
                {
                    self.request_close_tab(i);
                } else {
                    self.editor.switch_tab(i);
                    self.prompt_recover_if_needed(i);
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

    /// Returns the clamped cursor byte and whether the pointer hit actual text.
    fn mouse_byte(&self, pane: usize, col: usize, row: usize) -> (usize, bool) {
        let rect = self.pane_rect(pane);
        let state = &self.splits.panes[pane];
        let (idx, top, side) = if pane == self.splits.active_pane {
            (
                self.editor.active_tab_idx(),
                (self.scroll_row, self.scroll_subrow),
                self.scroll_col,
            )
        } else {
            (
                state.tab_idx,
                (state.scroll_row, state.scroll_subrow),
                state.scroll_col,
            )
        };
        let tab = &self.editor.tabs()[idx];
        let gutter = self.gutter_width_for_tab(idx).min(rect.width);
        let x = col.saturating_sub(rect.left + gutter);
        let y = row
            .saturating_sub(rect.top)
            .min(rect.height.saturating_sub(1));
        let cols = visual::wrap_cols_for_viewport(rect.width - gutter);
        let (line, sub) = if self.word_wrap {
            tab.offset_visual_rows(top, y as isize, cols)
        } else {
            (top.0.saturating_add(y), 0)
        };
        let Some(slice) = tab.buffer.line(line) else {
            return (tab.buffer.len_bytes(), false);
        };
        let text = std::borrow::Cow::from(slice);
        let content = text.trim_end_matches(LINE_BREAKS);
        let (byte, inside) = if self.word_wrap {
            let seg = visual::wrap_segments(content, cols).nth(sub);
            let beyond = line == tab.buffer.len_lines() - 1
                && top.0.saturating_add(y) >= line
                && tab.offset_visual_rows((line, sub), -(y as isize), cols) != top;
            let inside = seg.is_some_and(|s| x < s.col_width) && !beyond;
            (visual::byte_at_subrow_col(content, sub, x, cols), inside)
        } else {
            let byte = visual::byte_at_column(content, x + side);
            (byte, byte < content.len())
        };
        (
            tab.buffer.line_to_byte(line) + byte,
            inside && col >= rect.left + gutter,
        )
    }

    fn handle_ctrl_left_click(&mut self, col: usize, row: usize) -> bool {
        if self.quick_open.is_some()
            || self.prompt.is_some()
            || self.palette_open
            || self.which_key_open
            || self.search_state.is_some()
        {
            return false;
        }

        let left = self.text_area_left();
        let HitTarget::Pane(target_pane) = self.splits.hit_test(
            col,
            row,
            left,
            self.text_area_top(),
            (self.terminal_width as usize).saturating_sub(left),
            self.text_area_height(),
        ) else {
            return false;
        };
        let (head, hit) = self.mouse_byte(target_pane, col, row);
        if !hit {
            return false;
        }
        self.clear_pending_keys();
        self.focus = ActiveFocus::Editor;
        if self.splits.is_split && target_pane != self.splits.active_pane {
            self.switch_active_pane(target_pane);
        }
        self.editor.cursors_mut().set_single(head);
        self.mouse_dragging = false;
        self.sync_active_pane();
        self.request_definition_at_cursor();
        true
    }
}
