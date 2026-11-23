use std::path::PathBuf;

use crossterm::event::KeyEvent;
use lume_core::TextBuffer;
use lume_syntax::Language;

use crate::app::App;
use crate::input::key::typed_character;
use crate::prompt::{parse_goto_line, InputPrompt, PromptAction};
use crate::search::{SearchField, SearchMode, SearchState};

impl App {
    pub(crate) fn handle_prompt_key(
        &mut self,
        mut prompt: InputPrompt,
        action: Option<&str>,
        key: KeyEvent,
    ) -> std::io::Result<()> {
        if let PromptAction::ConfirmRecoverJournal { tab_idx } = prompt.action {
            match action {
                Some("recovery.recover") => {
                    self.editor.recover_tab_journal(tab_idx);
                    self.adjust_viewport();
                    self.update_syntax_highlights_for_tab(tab_idx);
                    self.set_status("Recovered changes from journal");
                    self.prompt_next_recovery_if_any();
                    return Ok(());
                }
                Some("recovery.discard") => {
                    self.editor.discard_tab_changes(tab_idx);
                    self.set_status("Discarded journal recovery");
                    self.prompt_next_recovery_if_any();
                    return Ok(());
                }
                Some("prompt.cancel") => {
                    if let Some(tab) = self.editor.tabs_mut().get_mut(tab_idx) {
                        tab.pending_recovery = None;
                    }
                    self.set_status("Cancelled");
                    self.prompt_next_recovery_if_any();
                    return Ok(());
                }
                _ => {
                    self.prompt = Some(prompt);
                    return Ok(());
                }
            }
        }

        if matches!(
            prompt.action,
            PromptAction::ConfirmQuit | PromptAction::ConfirmCloseTab { .. }
        ) {
            match action {
                Some("confirm.save") => {
                    match prompt.action {
                        PromptAction::ConfirmQuit => {
                            let mut all_saved = true;
                            for i in 0..self.editor.tabs().len() {
                                if self.editor.tabs()[i].is_dirty {
                                    if self.editor.tabs()[i].file_path.is_some() {
                                        if let Err(e) = self.editor.save_tab(i) {
                                            self.set_status(format!("Save error: {}", e));
                                            return Ok(());
                                        }
                                    } else {
                                        self.editor.switch_tab(i);
                                        self.prompt = Some(InputPrompt::save_as(None));
                                        all_saved = false;
                                        break;
                                    }
                                }
                            }
                            if all_saved {
                                self.running = false;
                            }
                        }
                        PromptAction::ConfirmCloseTab { tab_idx } => {
                            if self
                                .editor
                                .tabs()
                                .get(tab_idx)
                                .and_then(|t| t.file_path.as_ref())
                                .is_some()
                            {
                                if let Err(e) = self.editor.save_tab(tab_idx) {
                                    self.set_status(format!("Save error: {}", e));
                                    return Ok(());
                                }
                                self.close_tab_and_sync_splits(tab_idx);
                                self.set_status("Saved and closed tab");
                            } else {
                                self.editor.switch_tab(tab_idx);
                                self.prompt = Some(InputPrompt::save_as(None));
                            }
                        }
                        _ => {}
                    }
                    return Ok(());
                }
                Some("confirm.discard") => {
                    match prompt.action {
                        PromptAction::ConfirmQuit => {
                            self.editor.discard_all_journals();
                            self.running = false;
                        }
                        PromptAction::ConfirmCloseTab { tab_idx } => {
                            self.editor.discard_tab_changes(tab_idx);
                            self.close_tab_and_sync_splits(tab_idx);
                            self.set_status("Closed tab (changes discarded)");
                        }
                        _ => {}
                    }
                    return Ok(());
                }
                Some("prompt.cancel") => {
                    self.set_status("Cancelled");
                    return Ok(());
                }
                _ => {
                    self.prompt = Some(prompt);
                    return Ok(());
                }
            }
        }

        match action {
            Some("prompt.cancel") => {
                self.set_status("Cancelled");
            }
            Some("prompt.submit") => {
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
                                self.open_file_into_editor(&path, "Opened ", "Open error: ");
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
                                    let tab_id = self.editor.active_tab().id;
                                    self.invalidate_syntax_for_tab(tab_id);
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

                        let tab_id = self.editor.active_tab().id;
                        if let Some(lang) = Language::from_id(&chosen) {
                            self.editor
                                .set_language_override(Some(lang.name().to_string()));
                            self.invalidate_syntax_for_tab(tab_id);
                            self.set_status(format!("Language mode: {}", lang.name()));
                        } else if chosen.to_lowercase() == "plain text" {
                            self.editor.set_language_override(None);
                            self.invalidate_syntax_for_tab(tab_id);
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
                                Err(e) => self.set_status(format!("Create folder error: {}", e)),
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
                    PromptAction::CreateFile { parent_dir } => {
                        if !val.is_empty() {
                            match self.explorer.create_file(&parent_dir, &val) {
                                Ok(created) => self.open_file_into_editor(
                                    &created,
                                    "Created and opened file: ",
                                    "Created file but failed to open: ",
                                ),
                                Err(e) => self.set_status(format!("Create file error: {}", e)),
                            }
                        }
                    }
                    PromptAction::RenameItem {
                        source_path,
                        is_dir: _,
                    } => {
                        if !val.is_empty() {
                            match self.explorer.rename_item(&source_path, &val) {
                                Ok(renamed) => {
                                    self.handle_renamed_path_tabs(&source_path, &renamed);
                                    self.set_status(format!(
                                        "Renamed to: {}",
                                        renamed.file_name().and_then(|n| n.to_str()).unwrap_or("")
                                    ));
                                }
                                Err(e) => self.set_status(format!("Rename error: {}", e)),
                            }
                        }
                    }
                    PromptAction::GoToLine => {
                        let (curr_line_0, _) = self.editor.cursor_line_col();
                        let current_line_1 = curr_line_0 + 1;
                        if let Some((target_line_1, target_col_1)) =
                            parse_goto_line(&val, current_line_1)
                        {
                            let line_0 = target_line_1.saturating_sub(1);
                            let col_0 = target_col_1.saturating_sub(1);
                            self.editor.set_cursor_from_line_col(line_0, col_0, false);
                            let head = self.editor.cursors().primary().head;
                            self.editor.cursors_mut().set_single(head);
                            self.adjust_viewport();
                            let (actual_line_0, actual_col_0) = self.editor.cursor_line_col();
                            self.set_status(format!(
                                "Jumped to line {}, col {}",
                                actual_line_0 + 1,
                                actual_col_0 + 1
                            ));
                        } else {
                            self.set_status(format!("Invalid line[:col] format: '{}'", val));
                        }
                    }
                    PromptAction::ConfirmQuit => {
                        let mut all_saved = true;
                        for i in 0..self.editor.tabs().len() {
                            if self.editor.tabs()[i].is_dirty {
                                if self.editor.tabs()[i].file_path.is_some() {
                                    if let Err(e) = self.editor.save_tab(i) {
                                        self.set_status(format!("Save error: {}", e));
                                        return Ok(());
                                    }
                                } else {
                                    self.editor.switch_tab(i);
                                    self.prompt = Some(InputPrompt::save_as(None));
                                    all_saved = false;
                                    break;
                                }
                            }
                        }
                        if all_saved {
                            self.running = false;
                        }
                    }
                    PromptAction::ConfirmCloseTab { tab_idx } => {
                        if self
                            .editor
                            .tabs()
                            .get(tab_idx)
                            .and_then(|t| t.file_path.as_ref())
                            .is_some()
                        {
                            if let Err(e) = self.editor.save_tab(tab_idx) {
                                self.set_status(format!("Save error: {}", e));
                                return Ok(());
                            }
                            self.close_tab_and_sync_splits(tab_idx);
                            self.set_status("Saved and closed tab");
                        } else {
                            self.editor.switch_tab(tab_idx);
                            self.prompt = Some(InputPrompt::save_as(None));
                        }
                    }
                    PromptAction::ConfirmRecoverJournal { .. } => {}
                }
            }
            Some("prompt.complete") => {
                prompt.autocomplete();
                self.prompt = Some(prompt);
            }
            Some("field.backspace") => {
                prompt.handle_backspace();
                self.prompt = Some(prompt);
            }
            Some("field.left") => {
                prompt.move_left();
                self.prompt = Some(prompt);
            }
            Some("field.right") => {
                prompt.move_right();
                self.prompt = Some(prompt);
            }
            Some("list.up") => {
                if prompt.selected_suggestion > 0 {
                    prompt.selected_suggestion -= 1;
                }
                self.prompt = Some(prompt);
            }
            Some("list.down") => {
                if !prompt.suggestions.is_empty()
                    && prompt.selected_suggestion + 1 < prompt.suggestions.len()
                {
                    prompt.selected_suggestion += 1;
                }
                self.prompt = Some(prompt);
            }
            None => {
                if let Some(c) = typed_character(key) {
                    prompt.handle_char(c);
                }
                self.prompt = Some(prompt);
            }
            _ => {
                self.prompt = Some(prompt);
            }
        }
        Ok(())
    }

    pub(crate) fn handle_search_key(
        &mut self,
        mut state: SearchState,
        action: Option<&str>,
        key: KeyEvent,
    ) -> std::io::Result<()> {
        if action == Some("search.replace_all") {
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

        match action {
            Some("search.undo") => {
                if self.editor.undo() {
                    self.set_status("Undo");
                    state.match_count =
                        lume_core::search::count_matches(self.editor.buffer(), &state.query, false);
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
            }
            Some("search.redo") => {
                if self.editor.redo() {
                    self.set_status("Redo");
                    state.match_count =
                        lume_core::search::count_matches(self.editor.buffer(), &state.query, false);
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
            }
            Some("search.cancel") => {
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
            }
            Some("search.toggle_field") => {
                if state.mode == SearchMode::Replace {
                    state.toggle_field();
                }
                self.search_state = Some(state);
            }
            Some("search.next" | "search.previous") => {
                if !state.query.is_empty() {
                    let res = if action == Some("search.previous") {
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
            }
            Some("search.accept") => {
                if state.mode == SearchMode::Replace
                    && state.active_field == SearchField::Replacement
                {
                    let replaced =
                        self.editor
                            .replace_current(&state.query, &state.replacement, false);
                    state.match_count =
                        lume_core::search::count_matches(self.editor.buffer(), &state.query, false);
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
            }
            Some("field.left") => {
                state.move_left();
                self.search_state = Some(state);
            }
            Some("field.right") => {
                state.move_right();
                self.search_state = Some(state);
            }
            Some("field.home") => {
                state.move_home();
                self.search_state = Some(state);
            }
            Some("field.end") => {
                state.move_end();
                self.search_state = Some(state);
            }
            Some("field.backspace") => {
                state.handle_backspace();
                if state.active_field == SearchField::Query {
                    self.update_search_query(&mut state);
                }
                self.search_state = Some(state);
            }
            Some("field.delete") => {
                state.handle_delete();
                if state.active_field == SearchField::Query {
                    self.update_search_query(&mut state);
                }
                self.search_state = Some(state);
            }
            Some("search.find_mode") => {
                state.mode = SearchMode::Find;
                state.active_field = SearchField::Query;
                self.search_state = Some(state);
            }
            Some("search.replace_mode") => {
                state.mode = SearchMode::Replace;
                if state.query.is_empty() {
                    state.active_field = SearchField::Query;
                } else {
                    state.active_field = SearchField::Replacement;
                }
                self.search_state = Some(state);
            }
            None => {
                if let Some(c) = typed_character(key) {
                    state.handle_char(c);
                    if state.active_field == SearchField::Query {
                        self.update_search_query(&mut state);
                    }
                }
                self.search_state = Some(state);
            }
            _ => {
                self.search_state = Some(state);
            }
        }
        Ok(())
    }

    pub(crate) fn handle_palette_key(
        &mut self,
        action: Option<&str>,
        key: KeyEvent,
    ) -> std::io::Result<()> {
        match action {
            Some("list.cancel") => {
                self.palette_open = false;
            }
            Some("list.up") => {
                if self.palette_selected > 0 {
                    self.palette_selected -= 1;
                }
            }
            Some("list.down") => {
                let total = self.filtered_commands().len();
                if total > 0 && self.palette_selected + 1 < total {
                    self.palette_selected += 1;
                }
            }
            Some("list.accept") => {
                let commands = self.filtered_commands();
                if let Some(cmd) = commands.get(self.palette_selected) {
                    let id = cmd.id;
                    self.palette_open = false;
                    self.execute_command(id);
                } else {
                    self.palette_open = false;
                }
            }
            Some("field.backspace") => {
                self.palette_query.pop();
                self.palette_selected = 0;
            }
            None => {
                if let Some(c) = typed_character(key) {
                    self.palette_query.push(c);
                    self.palette_selected = 0;
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn handle_explorer_fallback(&mut self, key: KeyEvent) {
        if let Some(ch) = typed_character(key) {
            if ch.is_alphanumeric() || ch == '.' || ch == '_' {
                let h = self.text_area_height();
                self.explorer.jump_to_prefix(ch, h);
            }
        }
    }

    pub(crate) fn handle_quick_open_key(
        &mut self,
        mut state: crate::quick_open::QuickOpenState,
        action: Option<&str>,
        key: KeyEvent,
    ) -> std::io::Result<()> {
        let visible_rows =
            crate::quick_open::layout(self.terminal_width as usize, self.terminal_height as usize)
                .height
                .saturating_sub(crate::quick_open::HEADER_ROWS);

        match action {
            Some("list.cancel") => {
                self.quick_open = None;
                self.set_status("Cancelled");
            }
            Some("list.up") => {
                state.move_up();
                self.quick_open = Some(state);
            }
            Some("list.down") => {
                state.move_down(visible_rows);
                self.quick_open = Some(state);
            }
            Some("list.page_up") => {
                for _ in 0..10 {
                    state.move_up();
                }
                self.quick_open = Some(state);
            }
            Some("list.page_down") => {
                for _ in 0..10 {
                    state.move_down(visible_rows);
                }
                self.quick_open = Some(state);
            }
            Some("list.accept") => {
                if let Some(item) = state.selected_item() {
                    let full_path = item.full_path.clone();
                    self.quick_open = None;
                    self.open_file_into_editor(&full_path, "Opened ", "Open error: ");
                } else {
                    self.quick_open = None;
                }
            }
            Some("field.backspace") => {
                state.handle_backspace();
                self.quick_open = Some(state);
            }
            None => {
                if let Some(c) = typed_character(key) {
                    state.handle_char(c);
                }
                self.quick_open = Some(state);
            }
            _ => {
                self.quick_open = Some(state);
            }
        }
        Ok(())
    }
}
