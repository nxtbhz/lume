use std::path::PathBuf;

use crate::app::{ActiveFocus, App};
use crate::prompt::PromptAction;
use crate::search::{SearchField, SearchMode};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use lume_core::TextBuffer;
use lume_syntax::Language;

impl App {
    pub fn handle_key(&mut self, key: KeyEvent) -> std::io::Result<()> {
        if key.kind == KeyEventKind::Release {
            return Ok(());
        }

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);

        // 1. If Which-Key overlay is open, any key dismisses it
        if self.which_key_open {
            self.which_key_open = false;
            if key.code == KeyCode::Esc || key.code == KeyCode::F(1) {
                return Ok(());
            }
        }

        // 2. If an Input Prompt is open (Open File, Save As, Language Mode)
        if let Some(mut prompt) = self.prompt.take() {
            match key.code {
                KeyCode::Esc => {
                    self.set_status("Cancelled");
                }
                KeyCode::Enter => {
                    let val = prompt.value.trim().to_string();
                    match prompt.action {
                        PromptAction::OpenFile => {
                            if !val.is_empty() {
                                let raw_p = PathBuf::from(&val);
                                let path = if raw_p.is_absolute() {
                                    raw_p
                                } else if self.explorer.root.join(&raw_p).exists() {
                                    self.explorer.root.join(&raw_p)
                                } else {
                                    raw_p
                                };
                                if path.exists() {
                                    match self.editor.open_or_load(&path) {
                                        Ok(_) => {
                                            self.focus = ActiveFocus::Editor;
                                            self.set_status(format!("Opened {}", path.display()));
                                        }
                                        Err(e) => self.set_status(format!("Open error: {}", e)),
                                    }
                                } else {
                                    self.set_status(format!("File not found: {}", path.display()));
                                }
                            }
                        }
                        PromptAction::SaveFileAs => {
                            if !val.is_empty() {
                                let raw_p = PathBuf::from(&val);
                                let path = if raw_p.is_absolute() {
                                    raw_p
                                } else {
                                    self.explorer.root.join(raw_p)
                                };
                                match self.editor.save_as(&path) {
                                    Ok(()) => {
                                        self.explorer.refresh();
                                        self.set_status(format!("Saved to {}", path.display()));
                                    }
                                    Err(e) => self.set_status(format!("Save error: {}", e)),
                                }
                            }
                        }
                        PromptAction::SetLanguage => {
                            let chosen = if !prompt.suggestions.is_empty()
                                && prompt.selected_suggestion < prompt.suggestions.len()
                                && (val.is_empty()
                                    || prompt.suggestions[prompt.selected_suggestion]
                                        .to_lowercase()
                                        .starts_with(&val.to_lowercase()))
                            {
                                prompt.suggestions[prompt.selected_suggestion].clone()
                            } else {
                                val.clone()
                            };

                            if let Some(lang) = Language::from_id(&chosen) {
                                self.editor
                                    .set_language_override(Some(lang.name().to_string()));
                                self.syntax_cache.remove(&self.editor.active_tab().id);
                                self.syntax_in_flight.remove(&self.editor.active_tab().id);
                                self.set_status(format!("Language mode: {}", lang.name()));
                            } else if chosen.to_lowercase() == "plain text" {
                                self.editor.set_language_override(None);
                                self.syntax_cache.remove(&self.editor.active_tab().id);
                                self.syntax_in_flight.remove(&self.editor.active_tab().id);
                                self.set_status("Language mode: Plain Text");
                            } else {
                                self.set_status(format!("Unknown language: {}", chosen));
                            }
                        }
                        PromptAction::CreateFolder { parent_dir } => {
                            if !val.is_empty() {
                                match self.explorer.create_folder(&parent_dir, &val) {
                                    Ok(created) => {
                                        self.set_status(format!(
                                            "Created folder: {}",
                                            created.display()
                                        ));
                                    }
                                    Err(e) => {
                                        self.set_status(format!("Create folder error: {}", e))
                                    }
                                }
                            }
                        }
                        PromptAction::ConfirmDeleteFolder { path } => {
                            let should_delete = val.is_empty()
                                || val.eq_ignore_ascii_case("y")
                                || val.eq_ignore_ascii_case("yes");
                            if should_delete {
                                self.handle_deleted_folder_tabs(&path);
                                match self.explorer.delete_folder(&path) {
                                    Ok(deleted) => {
                                        self.set_status(format!(
                                            "Deleted folder: {}",
                                            deleted.display()
                                        ));
                                    }
                                    Err(e) => self.set_status(format!("Delete error: {}", e)),
                                }
                            } else {
                                self.set_status("Deletion cancelled");
                            }
                        }
                    }
                }
                KeyCode::Tab => {
                    prompt.autocomplete();
                    self.prompt = Some(prompt);
                }
                KeyCode::Backspace => {
                    prompt.handle_backspace();
                    self.prompt = Some(prompt);
                }
                KeyCode::Left => {
                    prompt.move_left();
                    self.prompt = Some(prompt);
                }
                KeyCode::Right => {
                    prompt.move_right();
                    self.prompt = Some(prompt);
                }
                KeyCode::Up => {
                    if prompt.selected_suggestion > 0 {
                        prompt.selected_suggestion -= 1;
                    }
                    self.prompt = Some(prompt);
                }
                KeyCode::Down => {
                    if !prompt.suggestions.is_empty()
                        && prompt.selected_suggestion + 1 < prompt.suggestions.len()
                    {
                        prompt.selected_suggestion += 1;
                    }
                    self.prompt = Some(prompt);
                }
                KeyCode::Char(c) if !ctrl && !alt => {
                    prompt.handle_char(c);
                    self.prompt = Some(prompt);
                }
                _ => {
                    self.prompt = Some(prompt);
                }
            }
            return Ok(());
        }

        // 3. If Search Bar is open (Find or Replace)
        if let Some(mut state) = self.search_state.take() {
            let is_ctrl_alt_enter = key.code == KeyCode::Enter
                && key.modifiers.contains(KeyModifiers::CONTROL)
                && key.modifiers.contains(KeyModifiers::ALT);

            if is_ctrl_alt_enter {
                if state.mode == SearchMode::Replace && !state.query.is_empty() {
                    let count = self
                        .editor
                        .replace_all(&state.query, &state.replacement, false);
                    state.match_count =
                        lume_core::search::count_matches(self.editor.buffer(), &state.query, false);
                    state.active_match_idx = 0;
                    self.set_status(format!("Substituídas {} ocorrências", count));
                    self.adjust_viewport();
                }
                self.search_state = Some(state);
                return Ok(());
            }

            match key.code {
                KeyCode::Char('z') if ctrl && !alt => {
                    if self.editor.undo() {
                        self.set_status("Undo");
                        state.match_count = lume_core::search::count_matches(
                            self.editor.buffer(),
                            &state.query,
                            false,
                        );
                        let cur_head = self.editor.cursors().primary().min();
                        state.active_match_idx = lume_core::search::find_matches_in_range(
                            self.editor.buffer(),
                            &state.query,
                            0,
                            cur_head,
                            false,
                        )
                        .len();
                        self.adjust_viewport();
                    }
                    self.search_state = Some(state);
                    return Ok(());
                }
                KeyCode::Char('y') if ctrl && !alt => {
                    if self.editor.redo() {
                        self.set_status("Redo");
                        state.match_count = lume_core::search::count_matches(
                            self.editor.buffer(),
                            &state.query,
                            false,
                        );
                        let cur_head = self.editor.cursors().primary().min();
                        state.active_match_idx = lume_core::search::find_matches_in_range(
                            self.editor.buffer(),
                            &state.query,
                            0,
                            cur_head,
                            false,
                        )
                        .len();
                        self.adjust_viewport();
                    }
                    self.search_state = Some(state);
                    return Ok(());
                }
                KeyCode::Esc => {
                    let max_bytes = self.editor.buffer().len_bytes();
                    let mut cur = state.initial_cursor;
                    cur.anchor = cur.anchor.min(max_bytes);
                    cur.head = cur.head.min(max_bytes);
                    self.editor.cursors_mut().set_cursors(vec![cur]);
                    self.scroll_row = state.initial_scroll.0;
                    self.scroll_col = state.initial_scroll.1;
                    if !state.query.is_empty() {
                        self.last_search_query = Some(state.query.clone());
                    }
                    self.adjust_viewport();
                    self.search_state = None;
                    return Ok(());
                }
                KeyCode::Tab | KeyCode::BackTab => {
                    if state.mode == SearchMode::Replace {
                        state.toggle_field();
                    }
                    self.search_state = Some(state);
                    return Ok(());
                }
                KeyCode::F(3) => {
                    if !state.query.is_empty() {
                        let res = if shift {
                            self.editor.find_prev(&state.query, false)
                        } else {
                            self.editor.find_next(&state.query, false)
                        };
                        if let Some((s, _e)) = res {
                            state.active_match_idx = lume_core::search::find_matches_in_range(
                                self.editor.buffer(),
                                &state.query,
                                0,
                                s,
                                false,
                            )
                            .len();
                            self.adjust_viewport();
                        }
                    }
                    self.search_state = Some(state);
                    return Ok(());
                }
                KeyCode::Enter => {
                    if shift {
                        if !state.query.is_empty() {
                            if let Some((s, _e)) = self.editor.find_prev(&state.query, false) {
                                state.active_match_idx = lume_core::search::find_matches_in_range(
                                    self.editor.buffer(),
                                    &state.query,
                                    0,
                                    s,
                                    false,
                                )
                                .len();
                                self.adjust_viewport();
                            }
                        }
                    } else if state.mode == SearchMode::Replace
                        && state.active_field == SearchField::Replacement
                    {
                        let replaced =
                            self.editor
                                .replace_current(&state.query, &state.replacement, false);
                        state.match_count = lume_core::search::count_matches(
                            self.editor.buffer(),
                            &state.query,
                            false,
                        );
                        if replaced {
                            self.set_status("Ocorrência substituída");
                        }
                        let cur_head = self.editor.cursors().primary().min();
                        state.active_match_idx = lume_core::search::find_matches_in_range(
                            self.editor.buffer(),
                            &state.query,
                            0,
                            cur_head,
                            false,
                        )
                        .len();
                        self.adjust_viewport();
                    } else if !state.query.is_empty() {
                        if let Some((s, _e)) = self.editor.find_next(&state.query, false) {
                            state.active_match_idx = lume_core::search::find_matches_in_range(
                                self.editor.buffer(),
                                &state.query,
                                0,
                                s,
                                false,
                            )
                            .len();
                            self.adjust_viewport();
                        }
                    }
                    self.search_state = Some(state);
                    return Ok(());
                }
                KeyCode::Left => {
                    state.move_left();
                    self.search_state = Some(state);
                    return Ok(());
                }
                KeyCode::Right => {
                    state.move_right();
                    self.search_state = Some(state);
                    return Ok(());
                }
                KeyCode::Home => {
                    state.move_home();
                    self.search_state = Some(state);
                    return Ok(());
                }
                KeyCode::End => {
                    state.move_end();
                    self.search_state = Some(state);
                    return Ok(());
                }
                KeyCode::Backspace => {
                    state.handle_backspace();
                    if state.active_field == SearchField::Query {
                        self.update_search_query(&mut state);
                    }
                    self.search_state = Some(state);
                    return Ok(());
                }
                KeyCode::Delete => {
                    state.handle_delete();
                    if state.active_field == SearchField::Query {
                        self.update_search_query(&mut state);
                    }
                    self.search_state = Some(state);
                    return Ok(());
                }
                KeyCode::Char('f') if ctrl && !alt => {
                    state.mode = SearchMode::Find;
                    state.active_field = SearchField::Query;
                    self.search_state = Some(state);
                    return Ok(());
                }
                KeyCode::Char('h') if ctrl && !alt => {
                    state.mode = SearchMode::Replace;
                    if state.query.is_empty() {
                        state.active_field = SearchField::Query;
                    } else {
                        state.active_field = SearchField::Replacement;
                    }
                    self.search_state = Some(state);
                    return Ok(());
                }
                KeyCode::Char(c) if !ctrl && !alt => {
                    state.handle_char(c);
                    if state.active_field == SearchField::Query {
                        self.update_search_query(&mut state);
                    }
                    self.search_state = Some(state);
                    return Ok(());
                }
                _ => {
                    self.search_state = Some(state);
                    return Ok(());
                }
            }
        }

        // 4. If Command Palette is open, route keys to palette
        if self.palette_open {
            match key.code {
                KeyCode::Esc => {
                    self.palette_open = false;
                }
                KeyCode::Up => {
                    if self.palette_selected > 0 {
                        self.palette_selected -= 1;
                    }
                }
                KeyCode::Down => {
                    let total = self.filtered_commands().len();
                    if total > 0 && self.palette_selected + 1 < total {
                        self.palette_selected += 1;
                    }
                }
                KeyCode::Enter => {
                    let commands = self.filtered_commands();
                    if let Some(cmd) = commands.get(self.palette_selected) {
                        let id = cmd.id;
                        self.palette_open = false;
                        self.execute_command(id);
                    } else {
                        self.palette_open = false;
                    }
                }
                KeyCode::Backspace => {
                    self.palette_query.pop();
                    self.palette_selected = 0;
                }
                KeyCode::Char(c) if !ctrl && !alt => {
                    self.palette_query.push(c);
                    self.palette_selected = 0;
                }
                _ => {}
            }
            return Ok(());
        }

        // 4. Global Shortcuts
        if ctrl && alt && (key.code == KeyCode::Char('v') || key.code == KeyCode::Char('V')) {
            self.execute_command("settings.toggle_modal");
            return Ok(());
        }

        if ctrl && !alt {
            match key.code {
                KeyCode::Char('q') => {
                    self.running = false;
                    return Ok(());
                }
                KeyCode::Char('p') => {
                    self.palette_open = true;
                    self.palette_query.clear();
                    self.palette_selected = 0;
                    return Ok(());
                }
                KeyCode::Char('o') => {
                    self.execute_command("file.open");
                    return Ok(());
                }
                KeyCode::Char('s') => {
                    if shift {
                        self.execute_command("file.save_as");
                    } else {
                        self.execute_command("file.save");
                    }
                    return Ok(());
                }
                KeyCode::Char('S') => {
                    self.execute_command("file.save_as");
                    return Ok(());
                }
                KeyCode::Char('b') | KeyCode::Char('e') => {
                    self.execute_command("view.toggle_explorer");
                    return Ok(());
                }
                KeyCode::Char('t') => {
                    self.execute_command("file.new_tab");
                    return Ok(());
                }
                KeyCode::Char('w') => {
                    self.execute_command("file.close_tab");
                    return Ok(());
                }
                KeyCode::Tab => {
                    self.editor.next_tab();
                    return Ok(());
                }
                KeyCode::Char('z') => {
                    self.execute_command("edit.undo");
                    return Ok(());
                }
                KeyCode::Char('y') | KeyCode::Char('r') => {
                    self.execute_command("edit.redo");
                    return Ok(());
                }
                KeyCode::Char('f') => {
                    self.execute_command("edit.find");
                    return Ok(());
                }
                KeyCode::Char('h') => {
                    self.execute_command("edit.replace");
                    return Ok(());
                }
                KeyCode::Char('m') => {
                    self.execute_command("navigation.match_bracket");
                    return Ok(());
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::F(1) => {
                self.which_key_open = true;
                return Ok(());
            }
            KeyCode::F(2) => {
                self.execute_command("view.toggle_explorer");
                return Ok(());
            }
            KeyCode::F(3) => {
                if let Some(ref q) = self.last_search_query.clone() {
                    if shift {
                        let _ = self.editor.find_prev(q, false);
                    } else {
                        let _ = self.editor.find_next(q, false);
                    }
                    self.adjust_viewport();
                }
                return Ok(());
            }
            KeyCode::Right if alt => {
                self.editor.next_tab();
                return Ok(());
            }
            KeyCode::Left if alt => {
                self.editor.prev_tab();
                return Ok(());
            }
            _ => {}
        }

        // 5. If File Explorer is Focused
        if self.focus == ActiveFocus::Explorer && self.explorer.is_visible {
            match key.code {
                KeyCode::Up => {
                    self.explorer.move_up();
                }
                KeyCode::Down => {
                    self.explorer.move_down(self.text_area_height());
                }
                KeyCode::PageUp => {
                    for _ in 0..self.text_area_height().max(1) {
                        self.explorer.move_up();
                    }
                }
                KeyCode::PageDown => {
                    for _ in 0..self.text_area_height().max(1) {
                        self.explorer.move_down(self.text_area_height());
                    }
                }
                KeyCode::Left => {
                    self.explorer.collapse_selected();
                }
                KeyCode::Right => {
                    self.explorer.expand_selected();
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if let Some(path) = self.explorer.activate_selected() {
                        match self.editor.open_or_load(&path) {
                            Ok(_) => {
                                self.focus = ActiveFocus::Editor;
                                self.adjust_viewport();
                                self.set_status(format!("Opened {}", path.display()));
                            }
                            Err(e) => self.set_status(format!("Open error: {}", e)),
                        }
                    }
                }
                KeyCode::Tab | KeyCode::Esc => {
                    self.focus = ActiveFocus::Editor;
                }
                KeyCode::Char('n') | KeyCode::Char('a') if !ctrl && !alt => {
                    self.execute_command("explorer.new_folder");
                }
                KeyCode::Char('d') if !ctrl && !alt => {
                    self.execute_command("explorer.delete_folder");
                }
                KeyCode::Delete => {
                    self.execute_command("explorer.delete_folder");
                }
                KeyCode::Char('r') if !ctrl && !alt => {
                    self.execute_command("explorer.refresh");
                }
                _ => {}
            }
            return Ok(());
        }

        // 6. Editor key handling (Modal or Modeless)
        if self.modal_enabled {
            self.handle_modal_key(key)?;
        } else {
            match (key.code, ctrl, alt) {
                (KeyCode::Left, false, false) => self.editor.move_left(shift),
                (KeyCode::Right, false, false) => self.editor.move_right(shift),
                (KeyCode::Up, false, false) => self.editor.move_up(shift),
                (KeyCode::Down, false, false) => self.editor.move_down(shift),
                (KeyCode::PageUp, false, false) => {
                    let h = self.text_area_height().max(1);
                    self.editor.page_up(h, shift);
                }
                (KeyCode::PageDown, false, false) => {
                    let h = self.text_area_height().max(1);
                    self.editor.page_down(h, shift);
                }
                (KeyCode::Home, false, false) => self.editor.move_home(shift),
                (KeyCode::End, false, false) => self.editor.move_end(shift),
                (KeyCode::Backspace, false, false) => {
                    let _ = self.editor.delete_backwards();
                }
                (KeyCode::Delete, false, false) => {
                    let _ = self.editor.delete_forwards();
                }
                (KeyCode::Enter, false, false) => {
                    let _ = self.editor.insert_newline();
                }
                (KeyCode::Tab, false, false) => {
                    let _ = self.editor.insert_str("    ");
                }
                (KeyCode::Char(c), false, false) => {
                    let _ = self.editor.insert_char(c);
                }
                _ => {}
            }
        }

        self.adjust_viewport();
        Ok(())
    }
}
