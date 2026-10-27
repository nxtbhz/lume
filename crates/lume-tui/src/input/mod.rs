pub mod key;
pub mod modal;
pub mod mouse;
pub mod overlays;

use std::io::Write;

use crossterm::event::{KeyEvent, MouseEvent};

use crate::app::App;

pub enum AppEvent {
    Input(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
    FocusLost,
}

impl App {
    pub fn handle_event<W: Write>(
        &mut self,
        event: AppEvent,
        writer: &mut W,
    ) -> std::io::Result<()> {
        match event {
            AppEvent::Input(key) => {
                self.handle_key(key)?;
                self.render(writer)?;
            }
            AppEvent::Mouse(mouse) => {
                self.handle_mouse(mouse)?;
                self.render(writer)?;
            }
            AppEvent::Resize(w, h) => {
                self.terminal_width = w;
                self.terminal_height = h;
                self.adjust_viewport();
                self.render(writer)?;
            }
            AppEvent::FocusLost => {
                if self.editor.is_dirty() && self.editor.file_path().is_some() {
                    if let Err(e) = self.editor.save_async() {
                        self.set_status(format!("Save error: {}", e));
                    }
                    let _ = self.poll_async_saves();
                    self.render(writer)?;
                }
            }
        }
        Ok(())
    }

    pub fn handle_tick<W: Write>(&mut self, writer: &mut W) -> std::io::Result<()> {
        let has_syntax_update = self.has_pending_syntax_response();
        let autosaved = self.editor.check_autosave();
        let has_save_event = self.poll_async_saves();
        if autosaved || has_syntax_update || has_save_event {
            self.render(writer)?;
        }
        Ok(())
    }
}
