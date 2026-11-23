use crossterm::event::KeyEvent;

use crate::app::App;
use crate::config::KeymapContext;
use crate::input::key::DispatchOutcome;
use crate::vim::VimMode;

impl App {
    /// Routes a key event directly through the current modal context (`Normal`/`Visual`/`Insert`).
    // Called directly by clipboard_tests; runtime input routes through App::handle_key.
    #[cfg_attr(not(test), expect(dead_code))]
    pub fn handle_modal_key(&mut self, key: KeyEvent) -> std::io::Result<()> {
        let ctx = match self.vim_state.mode {
            VimMode::Normal => KeymapContext::Normal,
            VimMode::Visual => KeymapContext::Visual,
            VimMode::Insert => KeymapContext::Insert,
        };
        if self.dispatch_keymap(ctx, key) == DispatchOutcome::Unmatched
            && ctx == KeymapContext::Insert
        {
            self.handle_text_editing_fallback(key);
        }
        Ok(())
    }
}
