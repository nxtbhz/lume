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
    pub shortcut: &'static str,
}

pub const COMMANDS: &[CommandItem] = &[
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
        id: "view.which_key",
        title: "View: Which-Key Shortcuts",
        shortcut: "F1",
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
        id: "app.quit",
        title: "App: Quit",
        shortcut: "Ctrl+Q",
    },
];

impl App {
    pub fn filtered_commands(&self) -> Vec<&'static CommandItem> {
        if self.palette_query.is_empty() {
            COMMANDS.iter().collect()
        } else {
            let q = self.palette_query.to_lowercase();
            COMMANDS
                .iter()
                .filter(|c| {
                    c.title.to_lowercase().contains(&q) || c.shortcut.to_lowercase().contains(&q)
                })
                .collect()
        }
    }

    pub fn execute_command(&mut self, id: &str) {
        match id {
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
                self.editor.close_tab(idx);
                self.set_status("Closed tab");
            }
            "file.next_tab" => self.editor.next_tab(),
            "file.prev_tab" => self.editor.prev_tab(),
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
            "edit.copy" => {
                if let Some(text) = self.editor.copy_selection() {
                    self.pending_osc52 = Some(lume_core::clipboard::osc52_sequence_auto(&text));
                    self.set_status("Copied to clipboard");
                }
            }
            "edit.cut" => {
                if let Some(text) = self.editor.cut_selection() {
                    self.pending_osc52 = Some(lume_core::clipboard::osc52_sequence_auto(&text));
                    self.set_status("Cut to clipboard");
                }
            }
            "edit.paste" => {
                if self.editor.paste() {
                    self.set_status("Pasted from clipboard");
                }
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
            "view.which_key" => {
                self.which_key_open = true;
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
            "app.quit" => {
                self.running = false;
            }
            _ => {}
        }
        self.adjust_viewport();
    }
}
