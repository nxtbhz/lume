use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::{ActiveFocus, App};

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
        if let Some(prompt) = self.prompt.take() {
            return self.handle_prompt_key(prompt, key);
        }

        // 3. If Search Bar is open (Find or Replace)
        if let Some(state) = self.search_state.take() {
            return self.handle_search_key(state, key);
        }

        // 4. If Command Palette is open, route keys to palette
        if self.palette_open {
            return self.handle_palette_key(key);
        }

        // 5. Global Shortcuts
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
                if self.focus == ActiveFocus::Explorer && self.explorer.is_visible {
                    self.execute_command("explorer.rename");
                } else {
                    self.execute_command("view.toggle_explorer");
                }
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

        // 6. If File Explorer is Focused
        if self.focus == ActiveFocus::Explorer && self.explorer.is_visible {
            return self.handle_explorer_key(key);
        }

        // 7. Editor key handling (Modal or Modeless)
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
