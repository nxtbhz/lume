use crate::app::{ActiveFocus, App};
use crate::prompt::InputPrompt;
use crate::search::SearchState;
use crate::vim::VimMode;
use lume_core::TextBuffer;
use lume_syntax::Language;

#[derive(Clone, Debug)]
pub struct CommandItem {
    pub id: &'static str,
    pub title: &'static str,
    // Kept for static default shortcut metadata inspected by tests; runtime rendering reads active keymaps.
    #[cfg_attr(not(test), expect(dead_code))]
    pub shortcut: &'static str,
}

pub const COMMANDS: &[CommandItem] = &[
    CommandItem {
        id: "file.quick_open",
        title: "File: Quick Open File...",
        shortcut: "Ctrl+P",
    },
    CommandItem {
        id: "view.command_palette",
        title: "View: Command Palette...",
        shortcut: "Ctrl+Shift+P / Alt+P",
    },
    CommandItem {
        id: "file.open",
        title: "File: Open File...",
        shortcut: "Ctrl+O",
    },
    CommandItem {
        id: "file.save",
        title: "File: Save",
        shortcut: "Ctrl+S",
    },
    CommandItem {
        id: "file.save_as",
        title: "File: Save As...",
        shortcut: "Ctrl+Shift+S",
    },
    CommandItem {
        id: "view.toggle_explorer",
        title: "View: Toggle File Explorer",
        shortcut: "Ctrl+B / F2",
    },
    CommandItem {
        id: "view.toggle_explorer_focus",
        title: "View: Switch Focus Between Editor and Explorer",
        shortcut: "F6",
    },
    CommandItem {
        id: "explorer.new_file",
        title: "Explorer: New File...",
        shortcut: "a in Explorer",
    },
    CommandItem {
        id: "explorer.new_folder",
        title: "Explorer: New Folder...",
        shortcut: "n in Explorer",
    },
    CommandItem {
        id: "explorer.rename",
        title: "Explorer: Rename Selected Item...",
        shortcut: "F2 / m in Explorer",
    },
    CommandItem {
        id: "explorer.delete_folder",
        title: "Explorer: Delete Selected Folder...",
        shortcut: "d in Explorer",
    },
    CommandItem {
        id: "explorer.refresh",
        title: "Explorer: Refresh Tree",
        shortcut: "r in Explorer",
    },
    CommandItem {
        id: "language.set_mode",
        title: "Language: Change Language Mode",
        shortcut: "Ctrl+K M",
    },
    CommandItem {
        id: "file.new_tab",
        title: "File: New Tab",
        shortcut: "Ctrl+T",
    },
    CommandItem {
        id: "file.close_tab",
        title: "File: Close Tab",
        shortcut: "Ctrl+W",
    },
    CommandItem {
        id: "file.next_tab",
        title: "File: Next Tab",
        shortcut: "Ctrl+Tab / Alt+Right",
    },
    CommandItem {
        id: "file.prev_tab",
        title: "File: Previous Tab",
        shortcut: "Alt+Left",
    },
    CommandItem {
        id: "edit.undo",
        title: "Edit: Undo",
        shortcut: "Ctrl+Z",
    },
    CommandItem {
        id: "edit.redo",
        title: "Edit: Redo",
        shortcut: "Ctrl+Y",
    },
    CommandItem {
        id: "edit.cut",
        title: "Edit: Cut",
        shortcut: "Ctrl+X",
    },
    CommandItem {
        id: "edit.copy",
        title: "Edit: Copy",
        shortcut: "Ctrl+C",
    },
    CommandItem {
        id: "edit.paste",
        title: "Edit: Paste",
        shortcut: "Ctrl+V",
    },
    CommandItem {
        id: "edit.find",
        title: "Edit: Find in Buffer...",
        shortcut: "Ctrl+F",
    },
    CommandItem {
        id: "edit.replace",
        title: "Edit: Replace in Buffer...",
        shortcut: "Ctrl+H",
    },
    CommandItem {
        id: "navigation.match_bracket",
        title: "Navigation: Jump to Matching Bracket",
        shortcut: "Ctrl+M",
    },
    CommandItem {
        id: "navigation.goto_line",
        title: "Navigation: Go to Line...",
        shortcut: "Ctrl+G",
    },
    CommandItem {
        id: "navigation.goto_definition",
        title: "Navigation: Go to Definition",
        shortcut: "F12",
    },
    CommandItem {
        id: "view.which_key",
        title: "View: Which-Key Shortcuts",
        shortcut: "F1",
    },
    CommandItem {
        id: "view.split_vertical",
        title: "View: Split Editor Vertically",
        shortcut: "Ctrl+K \\",
    },
    CommandItem {
        id: "view.split_horizontal",
        title: "View: Split Editor Horizontally",
        shortcut: "Ctrl+K -",
    },
    CommandItem {
        id: "view.focus_next_split",
        title: "View: Focus Next Split Pane",
        shortcut: "Ctrl+K W",
    },
    CommandItem {
        id: "view.close_split",
        title: "View: Close Active Split Pane",
        shortcut: "Ctrl+K Q",
    },
    CommandItem {
        id: "view.toggle_line_numbers",
        title: "View: Toggle Line Numbers",
        shortcut: "",
    },
    CommandItem {
        id: "settings.toggle_auto_pairs",
        title: "Settings: Toggle Auto-Closing Pairs",
        shortcut: "",
    },
    CommandItem {
        id: "settings.toggle_smart_indent",
        title: "Settings: Toggle Smart Indentation",
        shortcut: "",
    },
    CommandItem {
        id: "settings.toggle_smart_unindent",
        title: "Settings: Toggle Smart Unindent",
        shortcut: "",
    },
    CommandItem {
        id: "settings.toggle_modal",
        title: "Settings: Toggle Modal Mode (Vim Motions) [:set modal]",
        shortcut: "Ctrl+Alt+V",
    },
    CommandItem {
        id: "edit.select_all",
        title: "Edit: Select All",
        shortcut: "Ctrl+A",
    },
    CommandItem {
        id: "config.open",
        title: "Config: Open Configuration File",
        shortcut: "",
    },
    CommandItem {
        id: "config.reload",
        title: "Config: Reload Configuration",
        shortcut: "",
    },
    CommandItem {
        id: "app.quit",
        title: "App: Quit",
        shortcut: "Ctrl+Q",
    },
];

/// Action IDs bindable in `config.toml` that are not listed as Command Palette rows.
pub const EXTRA_BINDABLE_ACTIONS: &[&str] = &[
    ":set modal",
    ":set nomodal",
    ":set modal!",
    "search.find_next",
    "search.find_prev",
    "explorer.move_up",
    "explorer.move_down",
    "explorer.page_up",
    "explorer.page_down",
    "explorer.collapse",
    "explorer.expand",
    "explorer.activate",
    "explorer.focus_editor",
    "motion.left",
    "motion.right",
    "motion.up",
    "motion.down",
    "motion.word_left",
    "motion.word_right",
    "motion.word_forward",
    "motion.home",
    "motion.end",
    "motion.page_up",
    "motion.page_down",
    "motion.document_start",
    "motion.document_end",
    "motion.select_left",
    "motion.select_right",
    "motion.select_up",
    "motion.select_down",
    "motion.select_word_left",
    "motion.select_word_right",
    "motion.select_word_forward",
    "motion.select_home",
    "motion.select_end",
    "motion.select_page_up",
    "motion.select_page_down",
    "motion.select_document_start",
    "motion.select_document_end",
    "vim.normal_mode",
    "vim.insert_mode",
    "vim.append_mode",
    "vim.open_line_below",
    "vim.open_line_above",
    "vim.visual_mode",
    "vim.delete_char",
    "vim.delete_line",
    "vim.yank_line",
    "vim.paste",
    "vim.visual_delete",
    "vim.visual_yank",
];

/// Returns `true` if `id` is a recognized bindable action ID.
pub fn is_valid_action_id(id: &str) -> bool {
    COMMANDS.iter().any(|c| c.id == id) || EXTRA_BINDABLE_ACTIONS.contains(&id)
}

impl App {
    /// Returns the active primary shortcut label for `id` from `self.config.keymaps`.
    pub fn shortcut_label_for_command(&self, id: &str) -> String {
        self.config
            .keymaps
            .primary_shortcut_for(id)
            .unwrap_or_default()
    }

    pub fn filtered_commands(&self) -> Vec<&'static CommandItem> {
        if self.palette_query.is_empty() {
            COMMANDS.iter().collect()
        } else {
            let q = self.palette_query.to_lowercase();
            COMMANDS
                .iter()
                .filter(|c| {
                    c.title.to_lowercase().contains(&q)
                        || self
                            .config
                            .keymaps
                            .shortcuts_for(c.id)
                            .iter()
                            .any(|s| s.to_lowercase().contains(&q))
                })
                .collect()
        }
    }

    pub fn execute_command(&mut self, id: &str) {
        match id {
            "file.quick_open" => {
                self.open_quick_open();
            }
            "view.command_palette" => {
                self.open_command_palette();
            }
            "file.open" => {
                self.prompt = Some(InputPrompt::open_file());
            }
            "file.save" => {
                if self.editor.file_path().is_some() {
                    match self.editor.save() {
                        Ok(()) => self.set_status("File saved successfully"),
                        Err(e) => self.set_status(format!("Save error: {}", e)),
                    }
                } else {
                    self.prompt = Some(InputPrompt::save_as(None));
                }
            }
            "file.save_as" => {
                let default_name = self
                    .editor
                    .file_path()
                    .map(|p| p.to_string_lossy().to_string());
                self.prompt = Some(InputPrompt::save_as(default_name.as_deref()));
            }
            "view.toggle_explorer" => {
                self.explorer.toggle_visible();
                if self.explorer.is_visible {
                    self.focus = ActiveFocus::Explorer;
                } else {
                    self.focus = ActiveFocus::Editor;
                }
                self.set_status(format!(
                    "File Explorer: {}",
                    if self.explorer.is_visible {
                        "OPEN"
                    } else {
                        "CLOSED"
                    }
                ));
            }
            "view.toggle_explorer_focus" => {
                self.focus = match self.focus {
                    ActiveFocus::Editor => {
                        if !self.explorer.is_visible {
                            self.explorer.toggle_visible();
                        }
                        ActiveFocus::Explorer
                    }
                    ActiveFocus::Explorer => ActiveFocus::Editor,
                };
            }
            "explorer.new_file" => {
                let target = self.explorer.target_folder_for_creation();
                let display = target
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("workspace")
                    .to_string();
                self.prompt = Some(InputPrompt::create_file(target, &display));
            }
            "explorer.new_folder" => {
                let target = self.explorer.target_folder_for_creation();
                let display = target
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("workspace")
                    .to_string();
                self.prompt = Some(InputPrompt::create_folder(target, &display));
            }
            "explorer.rename" => {
                if let Some(item) = self.explorer.selected_item() {
                    let root_canon = self
                        .explorer
                        .root
                        .canonicalize()
                        .unwrap_or_else(|_| self.explorer.root.clone());
                    let item_canon = item
                        .path
                        .canonicalize()
                        .unwrap_or_else(|_| item.path.clone());
                    if item_canon == root_canon {
                        self.set_status("Cannot rename workspace root directory");
                    } else {
                        self.prompt = Some(InputPrompt::rename_item(
                            item.path.clone(),
                            &item.name,
                            item.is_dir,
                        ));
                    }
                } else {
                    self.set_status("No item selected to rename");
                }
            }
            "explorer.delete_folder" => {
                if let Some(item) = self.explorer.selected_item() {
                    if item.is_dir {
                        self.prompt = Some(InputPrompt::confirm_delete_folder(item.path.clone()));
                    } else {
                        self.set_status("Selected item is not a folder (use delete on folders)");
                    }
                }
            }
            "explorer.refresh" => {
                self.explorer.refresh();
                self.set_status("File Explorer refreshed");
            }
            "language.set_mode" => {
                let langs: Vec<&'static str> = Language::all().iter().map(|l| l.name()).collect();
                self.prompt = Some(InputPrompt::set_language(&langs));
            }
            "file.new_tab" => {
                let idx = self.editor.new_tab();
                self.set_status(format!("Opened new Tab {}", idx + 1));
            }
            "file.close_tab" => {
                let idx = self.editor.active_tab_idx();
                self.request_close_tab(idx);
            }
            "file.next_tab" => {
                self.editor.next_tab();
                self.prompt_recover_if_needed(self.editor.active_tab_idx());
            }
            "file.prev_tab" => {
                self.editor.prev_tab();
                self.prompt_recover_if_needed(self.editor.active_tab_idx());
            }
            "edit.undo" => {
                if self.editor.undo() {
                    self.set_status("Undo");
                }
            }
            "edit.redo" => {
                if self.editor.redo() {
                    self.set_status("Redo");
                }
            }
            "edit.copy" | "edit.cut" => {
                let cut = id == "edit.cut";
                let linewise = !self
                    .editor
                    .cursors()
                    .cursors()
                    .iter()
                    .any(|c| c.is_selection());
                let text = if cut {
                    self.editor.cut_selection()
                } else {
                    self.editor.copy_selection()
                };
                if let Some(text) = text {
                    self.pending_osc52 = Some(lume_core::clipboard::osc52_sequence_auto(&text));
                    if self.modal_enabled {
                        self.vim_state.yank(text, linewise);
                        if cut && self.vim_state.mode == crate::vim::VimMode::Visual {
                            self.vim_state.set_mode(crate::vim::VimMode::Normal);
                        }
                    }
                    if cut {
                        self.syntax_cache.remove(&self.editor.active_tab().id);
                        self.adjust_viewport();
                        self.set_status("Cut to Lume clipboard");
                    } else {
                        self.set_status("Copied to Lume clipboard");
                    }
                }
            }
            "edit.paste" => {
                if let Some(text) = self.editor.clipboard().map(str::to_owned) {
                    if self.paste_into_editor(&text) {
                        self.set_status("Pasted from Lume clipboard");
                    }
                }
            }
            "edit.select_all" => {
                self.editor.select_all();
                self.set_status("Selected all");
            }
            "edit.find" => {
                let cur = *self.editor.cursors().primary();
                let scroll = (self.scroll_row, self.scroll_col);
                let initial_query = if cur.is_selection() {
                    let sel = self.editor.buffer().slice_to_cow(cur.min(), cur.max());
                    if !sel.contains('\n') && !sel.is_empty() {
                        Some(sel.to_string())
                    } else {
                        self.last_search_query.clone()
                    }
                } else {
                    self.last_search_query.clone()
                };
                let mut state = SearchState::new_find(cur, scroll, initial_query.as_deref());
                if !state.query.is_empty() {
                    state.match_count =
                        lume_core::search::count_matches(self.editor.buffer(), &state.query, false);
                    if let Some((s, _e)) = self.editor.find_next(&state.query, false) {
                        state.active_match_idx = lume_core::search::find_matches_in_range(
                            self.editor.buffer(),
                            &state.query,
                            0,
                            s,
                            false,
                        )
                        .len();
                    }
                    self.adjust_viewport();
                }
                self.search_state = Some(state);
            }
            "edit.replace" => {
                let cur = *self.editor.cursors().primary();
                let scroll = (self.scroll_row, self.scroll_col);
                let initial_query = if cur.is_selection() {
                    let sel = self.editor.buffer().slice_to_cow(cur.min(), cur.max());
                    if !sel.contains('\n') && !sel.is_empty() {
                        Some(sel.to_string())
                    } else {
                        self.last_search_query.clone()
                    }
                } else {
                    self.last_search_query.clone()
                };
                let mut state = SearchState::new_replace(cur, scroll, initial_query.as_deref());
                if !state.query.is_empty() {
                    state.match_count =
                        lume_core::search::count_matches(self.editor.buffer(), &state.query, false);
                    if let Some((s, _e)) = self.editor.find_next(&state.query, false) {
                        state.active_match_idx = lume_core::search::find_matches_in_range(
                            self.editor.buffer(),
                            &state.query,
                            0,
                            s,
                            false,
                        )
                        .len();
                    }
                    self.adjust_viewport();
                }
                self.search_state = Some(state);
            }
            "navigation.match_bracket" => {
                if self.editor.jump_to_matching_bracket() {
                    self.adjust_viewport();
                    self.set_status("Jumped to matching bracket");
                } else {
                    self.set_status("No matching bracket found");
                }
            }
            "navigation.goto_line" => {
                self.prompt = Some(InputPrompt::goto_line());
            }
            "navigation.goto_definition" => {
                self.request_definition_at_cursor();
            }
            "view.which_key" => {
                self.which_key_open = true;
            }
            "view.split_vertical" => {
                let tab_idx = self.editor.active_tab_idx();
                let scroll = (self.scroll_row, self.scroll_col);
                let cursors = self.editor.cursors().clone();
                self.splits.split_vertical(tab_idx, scroll, cursors);
                self.set_status("Split editor vertically");
                return;
            }
            "view.split_horizontal" => {
                let tab_idx = self.editor.active_tab_idx();
                let scroll = (self.scroll_row, self.scroll_col);
                let cursors = self.editor.cursors().clone();
                self.splits.split_horizontal(tab_idx, scroll, cursors);
                self.set_status("Split editor horizontally");
                return;
            }
            "view.focus_next_split" => {
                if self.splits.is_split {
                    let next = if self.splits.active_pane == 0 { 1 } else { 0 };
                    self.switch_active_pane(next);
                    self.set_status(format!("Focused split pane {}", next + 1));
                }
                return;
            }
            "view.close_split" => {
                if self.splits.is_split {
                    self.sync_active_pane();
                    self.splits.close_active();
                    let remaining = self.splits.panes[0].clone();
                    let tab_idx = remaining
                        .tab_idx
                        .min(self.editor.tabs().len().saturating_sub(1));
                    self.editor.switch_tab(tab_idx);
                    self.scroll_row = remaining.scroll_row;
                    self.scroll_col = remaining.scroll_col;
                    let mut cursors = remaining.cursors;
                    Self::clamp_cursors_to_buffer(&mut cursors, self.editor.buffer());
                    *self.editor.cursors_mut() = cursors;
                    self.set_status("Closed split pane");
                }
                return;
            }
            "view.toggle_line_numbers" => {
                self.show_line_numbers = !self.show_line_numbers;
                self.set_status(format!(
                    "Line numbers: {}",
                    if self.show_line_numbers { "ON" } else { "OFF" }
                ));
            }
            "settings.toggle_auto_pairs" => {
                let new_val = !self.editor.config().auto_pairs;
                self.editor.config_mut().auto_pairs = new_val;
                let status = if new_val { "Enabled" } else { "Disabled" };
                self.set_status(format!("Auto-closing pairs: {}", status));
            }
            "settings.toggle_smart_indent" => {
                let new_val = !self.editor.config().smart_indent;
                self.editor.config_mut().smart_indent = new_val;
                let status = if new_val { "Enabled" } else { "Disabled" };
                self.set_status(format!("Smart indentation: {}", status));
            }
            "settings.toggle_smart_unindent" => {
                let new_val = !self.editor.config().smart_unindent;
                self.editor.config_mut().smart_unindent = new_val;
                let status = if new_val { "Enabled" } else { "Disabled" };
                self.set_status(format!("Smart unindent: {}", status));
            }
            ":set modal" => {
                self.modal_enabled = true;
                self.vim_state.set_mode(VimMode::Normal);
                self.set_status("Modal mode (Vim Motions): ON [NORMAL]");
            }
            ":set nomodal" => {
                self.modal_enabled = false;
                self.vim_state.set_mode(VimMode::Normal);
                self.set_status("Modal mode (Vim Motions): OFF");
            }
            "settings.toggle_modal" | ":set modal!" => {
                self.modal_enabled = !self.modal_enabled;
                if self.modal_enabled {
                    self.vim_state.set_mode(VimMode::Normal);
                    self.set_status("Modal mode (Vim Motions): ON [NORMAL]");
                } else {
                    self.vim_state.set_mode(VimMode::Normal);
                    self.set_status("Modal mode (Vim Motions): OFF");
                }
            }
            "config.open" => {
                let Some(path) = self
                    .config_path
                    .clone()
                    .or_else(|| crate::config::resolve_config_path(None, false))
                else {
                    self.set_status("Could not determine configuration file path");
                    return;
                };
                self.config_path = Some(path.clone());
                if !path.exists() {
                    if let Some(parent) = path.parent() {
                        if let Err(err) = std::fs::create_dir_all(parent) {
                            self.set_status(format!("Config error: {err}"));
                            return;
                        }
                    }
                    if let Err(err) = std::fs::write(&path, crate::config::default_config_toml()) {
                        self.set_status(format!("Config error: {err}"));
                        return;
                    }
                }
                let target_canon = path.canonicalize().unwrap_or_else(|_| path.clone());
                if let Some(existing_idx) = self.editor.tabs().iter().position(|tab| {
                    tab.file_path.as_ref().is_some_and(|p| {
                        p.canonicalize().unwrap_or_else(|_| p.clone()) == target_canon
                    })
                }) {
                    self.editor.switch_tab(existing_idx);
                    self.focus = ActiveFocus::Editor;
                    self.set_status(format!("Switched to config {}", path.display()));
                } else {
                    self.open_file_into_editor(&path, "Opened config ", "Config open error: ");
                }
            }
            "config.reload" => {
                let Some(path) = self
                    .config_path
                    .clone()
                    .or_else(|| crate::config::resolve_config_path(None, false))
                else {
                    self.set_status("Config error: no configuration file path configured");
                    return;
                };
                match crate::config::AppConfig::load_from_path(&path, is_valid_action_id) {
                    Ok(cfg) => {
                        self.apply_config(cfg);
                        self.config_path = Some(path);
                        self.set_status("Configuration reloaded");
                    }
                    Err(err) => {
                        self.set_status(format!("Config error: {err}"));
                    }
                }
            }
            "search.find_next" => {
                if let Some(q) = self.last_search_query.clone() {
                    let _ = self.editor.find_next(&q, false);
                }
            }
            "search.find_prev" => {
                if let Some(q) = self.last_search_query.clone() {
                    let _ = self.editor.find_prev(&q, false);
                }
            }
            "explorer.move_up" => {
                self.explorer.move_up();
                return;
            }
            "explorer.move_down" => {
                let h = self.text_area_height();
                self.explorer.move_down(h);
                return;
            }
            "explorer.page_up" => {
                for _ in 0..self.text_area_height().max(1) {
                    self.explorer.move_up();
                }
                return;
            }
            "explorer.page_down" => {
                let h = self.text_area_height();
                for _ in 0..h.max(1) {
                    self.explorer.move_down(h);
                }
                return;
            }
            "explorer.collapse" => {
                self.explorer.collapse_selected();
                return;
            }
            "explorer.expand" => {
                self.explorer.expand_selected();
                return;
            }
            "explorer.activate" => {
                if let Some(path) = self.explorer.activate_selected() {
                    self.open_file_into_editor(&path, "Opened ", "Open error: ");
                }
                return;
            }
            "explorer.focus_editor" => {
                self.focus = ActiveFocus::Editor;
                return;
            }
            "motion.left" => self.editor.move_left(false),
            "motion.right" => self.editor.move_right(false),
            "motion.up" => self.editor.move_up(false),
            "motion.down" => self.editor.move_down(false),
            "motion.word_left" => self.editor.move_word_backward(false),
            "motion.word_right" => self.editor.move_word_end_forward(false),
            "motion.word_forward" => self.editor.move_word_forward(false),
            "motion.home" => self.editor.move_home(false),
            "motion.end" => self.editor.move_end(false),
            "motion.page_up" => {
                let h = self.text_area_height().max(1);
                self.editor.page_up(h, false);
            }
            "motion.page_down" => {
                let h = self.text_area_height().max(1);
                self.editor.page_down(h, false);
            }
            "motion.document_start" => self.editor.move_to_start(false),
            "motion.document_end" => self.editor.move_to_end(false),
            "motion.select_left" => self.editor.move_left(true),
            "motion.select_right" => self.editor.move_right(true),
            "motion.select_up" => self.editor.move_up(true),
            "motion.select_down" => self.editor.move_down(true),
            "motion.select_word_left" => self.editor.move_word_backward(true),
            "motion.select_word_right" => self.editor.move_word_end_forward(true),
            "motion.select_word_forward" => self.editor.move_word_forward(true),
            "motion.select_home" => self.editor.move_home(true),
            "motion.select_end" => self.editor.move_end(true),
            "motion.select_page_up" => {
                let h = self.text_area_height().max(1);
                self.editor.page_up(h, true);
            }
            "motion.select_page_down" => {
                let h = self.text_area_height().max(1);
                self.editor.page_down(h, true);
            }
            "motion.select_document_start" => self.editor.move_to_start(true),
            "motion.select_document_end" => self.editor.move_to_end(true),
            "vim.normal_mode" => {
                self.vim_state.clear_pending();
                self.vim_state.set_mode(VimMode::Normal);
                let head = self.editor.cursors().primary().head;
                self.editor.cursors_mut().set_single(head);
            }
            "vim.insert_mode" => {
                self.vim_state.set_mode(VimMode::Insert);
            }
            "vim.append_mode" => {
                self.editor.move_right(false);
                self.vim_state.set_mode(VimMode::Insert);
            }
            "vim.open_line_below" => {
                self.editor.move_end(false);
                let _ = self.editor.insert_newline();
                self.vim_state.set_mode(VimMode::Insert);
            }
            "vim.open_line_above" => {
                self.editor.move_home(false);
                let _ = self.editor.insert_newline();
                self.editor.move_up(false);
                self.vim_state.set_mode(VimMode::Insert);
            }
            "vim.visual_mode" => {
                self.vim_state.set_mode(VimMode::Visual);
            }
            "vim.delete_char" => {
                let cur = *self.editor.cursors().primary();
                if cur.is_selection() {
                    let text = self
                        .editor
                        .buffer()
                        .slice_to_cow(cur.min(), cur.max())
                        .into_owned();
                    self.vim_state.yank(text, false);
                } else if cur.head < self.editor.buffer().len_bytes() {
                    let char_idx = self.editor.buffer().byte_to_char(cur.head);
                    let next_byte = self.editor.buffer().char_to_byte(char_idx + 1);
                    let text = self
                        .editor
                        .buffer()
                        .slice_to_cow(cur.head, next_byte)
                        .into_owned();
                    self.vim_state.yank(text, false);
                }
                let _ = self.editor.delete_forwards();
            }
            "vim.delete_line" => {
                if let Ok(deleted) = self.editor.delete_line() {
                    self.vim_state.yank(deleted.clone(), true);
                    self.editor.set_clipboard(deleted.clone());
                    self.pending_osc52 = Some(lume_core::clipboard::osc52_sequence_auto(&deleted));
                }
            }
            "vim.yank_line" => {
                let yanked = self.editor.yank_line();
                self.vim_state.yank(yanked.clone(), true);
                self.editor.set_clipboard(yanked.clone());
                self.pending_osc52 = Some(lume_core::clipboard::osc52_sequence_auto(&yanked));
                self.set_status("1 line yanked");
            }
            "vim.paste" => {
                if let Some((text, linewise)) = self.vim_state.yank_register.clone() {
                    let _ = self.editor.paste_yank(&text, linewise);
                }
            }
            "vim.visual_delete" => {
                let cur = *self.editor.cursors().primary();
                if cur.is_selection() {
                    let text = self
                        .editor
                        .buffer()
                        .slice_to_cow(cur.min(), cur.max())
                        .into_owned();
                    self.vim_state.yank(text.clone(), false);
                    self.editor.set_clipboard(text.clone());
                    self.pending_osc52 = Some(lume_core::clipboard::osc52_sequence_auto(&text));
                }
                let _ = self.editor.delete_forwards();
                self.vim_state.set_mode(VimMode::Normal);
            }
            "vim.visual_yank" => {
                let cur = *self.editor.cursors().primary();
                if cur.is_selection() {
                    let text = self
                        .editor
                        .buffer()
                        .slice_to_cow(cur.min(), cur.max())
                        .into_owned();
                    self.vim_state.yank(text.clone(), false);
                    self.editor.set_clipboard(text.clone());
                    self.pending_osc52 = Some(lume_core::clipboard::osc52_sequence_auto(&text));
                }
                let head = self.editor.cursors().primary().head;
                self.editor.cursors_mut().set_single(head);
                self.vim_state.set_mode(VimMode::Normal);
                self.set_status("Yanked selection");
            }
            "app.quit" => {
                if self.editor.is_any_dirty() {
                    self.prompt = Some(InputPrompt::confirm_quit());
                } else {
                    self.running = false;
                }
            }
            _ => {}
        }
        self.adjust_viewport();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::keymap::{KeymapContext, KeymapTable};
    use crate::config::KeymapPreset;

    #[test]
    fn all_default_minimal_and_vim_preset_actions_are_registered() {
        for preset in [
            KeymapPreset::Default,
            KeymapPreset::Minimal,
            KeymapPreset::Vim,
        ] {
            let table = KeymapTable::from_preset(preset);
            for ctx in KeymapContext::COMMAND_CONTEXTS {
                for (_seq, action) in table.entries_for(ctx) {
                    assert!(
                        is_valid_action_id(action),
                        "Unknown action `{action}` in preset {preset:?} ({ctx:?})"
                    );
                }
            }
        }
    }

    #[test]
    fn execute_command_supports_motion_and_vim_action_ids() {
        let mut app = App::new(None).unwrap();
        app.execute_command(":set modal");
        app.execute_command("vim.insert_mode");
        assert_eq!(app.vim_state.mode, VimMode::Insert);
        app.execute_command("vim.normal_mode");
        assert_eq!(app.vim_state.mode, VimMode::Normal);
    }
}
