use crate::app::App;
use crate::vim::VimMode;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use lume_core::TextBuffer;

impl App {
    pub fn handle_modal_key(&mut self, key: KeyEvent) -> std::io::Result<()> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);

        match self.vim_state.mode {
            VimMode::Insert => match (key.code, ctrl, alt) {
                (KeyCode::Esc, false, false) => {
                    self.vim_state.set_mode(VimMode::Normal);
                    let head = self.editor.cursors().primary().head;
                    self.editor.cursors_mut().set_single(head);
                }
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
            },
            VimMode::Visual => {
                if let Some(op) = self.vim_state.clear_pending() {
                    if op == 'g' && key.code == KeyCode::Char('g') {
                        self.editor.move_to_start(true);
                        return Ok(());
                    }
                }

                if !ctrl && !alt {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('v') => {
                            let head = self.editor.cursors().primary().head;
                            self.editor.cursors_mut().set_single(head);
                            self.vim_state.set_mode(VimMode::Normal);
                        }
                        KeyCode::Char('h') | KeyCode::Left => self.editor.move_left(true),
                        KeyCode::Char('j') | KeyCode::Down => self.editor.move_down(true),
                        KeyCode::Char('k') | KeyCode::Up => self.editor.move_up(true),
                        KeyCode::Char('l') | KeyCode::Right => self.editor.move_right(true),
                        KeyCode::Char('w') => self.editor.move_word_forward(true),
                        KeyCode::Char('b') => self.editor.move_word_backward(true),
                        KeyCode::Char('0') | KeyCode::Home => self.editor.move_home(true),
                        KeyCode::Char('$') | KeyCode::End => self.editor.move_end(true),
                        KeyCode::Char('G') => self.editor.move_to_end(true),
                        KeyCode::Char('g') => {
                            self.vim_state.set_pending('g');
                        }
                        KeyCode::Char('d') | KeyCode::Char('x') => {
                            let cur = *self.editor.cursors().primary();
                            if cur.is_selection() {
                                let text = self
                                    .editor
                                    .buffer()
                                    .slice_to_cow(cur.min(), cur.max())
                                    .into_owned();
                                self.vim_state.yank(text, false);
                            }
                            let _ = self.editor.delete_forwards();
                            self.vim_state.set_mode(VimMode::Normal);
                        }
                        KeyCode::Char('y') => {
                            let cur = *self.editor.cursors().primary();
                            if cur.is_selection() {
                                let text = self
                                    .editor
                                    .buffer()
                                    .slice_to_cow(cur.min(), cur.max())
                                    .into_owned();
                                self.vim_state.yank(text, false);
                            }
                            let head = self.editor.cursors().primary().head;
                            self.editor.cursors_mut().set_single(head);
                            self.vim_state.set_mode(VimMode::Normal);
                            self.set_status("Yanked selection");
                        }
                        _ => {}
                    }
                }
            }
            VimMode::Normal => {
                if let Some(op) = self.vim_state.clear_pending() {
                    match (op, key.code) {
                        ('g', KeyCode::Char('g')) => {
                            self.editor.move_to_start(false);
                            return Ok(());
                        }
                        ('d', KeyCode::Char('d')) => {
                            if let Ok(deleted) = self.editor.delete_line() {
                                self.vim_state.yank(deleted, true);
                            }
                            return Ok(());
                        }
                        ('y', KeyCode::Char('y')) => {
                            let yanked = self.editor.yank_line();
                            self.vim_state.yank(yanked, true);
                            self.set_status("1 line yanked");
                            return Ok(());
                        }
                        _ => {}
                    }
                }

                if !ctrl && !alt {
                    match key.code {
                        KeyCode::Esc => {
                            self.vim_state.clear_pending();
                            let head = self.editor.cursors().primary().head;
                            self.editor.cursors_mut().set_single(head);
                        }
                        KeyCode::Char('g') => {
                            self.vim_state.set_pending('g');
                        }
                        KeyCode::Char('d') => {
                            self.vim_state.set_pending('d');
                        }
                        KeyCode::Char('y') => {
                            self.vim_state.set_pending('y');
                        }
                        KeyCode::Char('h') | KeyCode::Left => self.editor.move_left(false),
                        KeyCode::Char('j') | KeyCode::Down => self.editor.move_down(false),
                        KeyCode::Char('k') | KeyCode::Up => self.editor.move_up(false),
                        KeyCode::Char('l') | KeyCode::Right => self.editor.move_right(false),
                        KeyCode::Char('w') => self.editor.move_word_forward(false),
                        KeyCode::Char('b') => self.editor.move_word_backward(false),
                        KeyCode::Char('0') | KeyCode::Home => self.editor.move_home(false),
                        KeyCode::Char('$') | KeyCode::End => self.editor.move_end(false),
                        KeyCode::Char('G') => self.editor.move_to_end(false),
                        KeyCode::PageUp => {
                            let h = self.text_area_height().max(1);
                            self.editor.page_up(h, false);
                        }
                        KeyCode::PageDown => {
                            let h = self.text_area_height().max(1);
                            self.editor.page_down(h, false);
                        }
                        KeyCode::Char('x') => {
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
                        KeyCode::Char('p') => {
                            if let Some((ref text, linewise)) = self.vim_state.yank_register.clone()
                            {
                                let _ = self.editor.paste_yank(text, linewise);
                            }
                        }
                        KeyCode::Char('u') => {
                            self.execute_command("edit.undo");
                        }
                        KeyCode::Char('i') => {
                            self.vim_state.set_mode(VimMode::Insert);
                        }
                        KeyCode::Char('a') => {
                            self.editor.move_right(false);
                            self.vim_state.set_mode(VimMode::Insert);
                        }
                        KeyCode::Char('o') => {
                            self.editor.move_end(false);
                            let _ = self.editor.insert_newline();
                            self.vim_state.set_mode(VimMode::Insert);
                        }
                        KeyCode::Char('O') => {
                            self.editor.move_home(false);
                            let _ = self.editor.insert_newline();
                            self.editor.move_up(false);
                            self.vim_state.set_mode(VimMode::Insert);
                        }
                        KeyCode::Char('v') => {
                            self.vim_state.set_mode(VimMode::Visual);
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(())
    }
}
