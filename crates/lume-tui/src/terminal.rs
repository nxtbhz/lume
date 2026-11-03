use std::io::stdout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crossterm::{
    cursor::{Hide, Show},
    event::{
        DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
        EnableFocusChange, EnableMouseCapture, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    style::ResetColor,
    terminal::{
        disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};

pub(crate) fn requested_keyboard_enhancement_flags() -> KeyboardEnhancementFlags {
    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
        | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
}

pub(crate) fn should_enable_keyboard_enhancement(result: std::io::Result<bool>) -> bool {
    result.unwrap_or(false)
}

pub(crate) fn take_keyboard_enhancement(state: &AtomicBool) -> bool {
    state.swap(false, Ordering::SeqCst)
}

pub struct TerminalGuard {
    keyboard_enhancement_enabled: Arc<AtomicBool>,
}

impl TerminalGuard {
    pub fn init() -> Result<Self, std::io::Error> {
        let keyboard_enhancement_enabled = Arc::new(AtomicBool::new(false));
        let panic_enhancement = Arc::clone(&keyboard_enhancement_enabled);

        let default_panic = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let mut out = stdout();
            if take_keyboard_enhancement(&panic_enhancement) {
                let _ = execute!(out, PopKeyboardEnhancementFlags);
            }
            let _ = execute!(
                out,
                Show,
                ResetColor,
                DisableFocusChange,
                DisableMouseCapture,
                DisableBracketedPaste,
                LeaveAlternateScreen
            );
            let _ = disable_raw_mode();
            default_panic(info);
        }));

        enable_raw_mode()?;
        let mut out = stdout();
        execute!(
            out,
            EnterAlternateScreen,
            Hide,
            EnableMouseCapture,
            EnableFocusChange,
            EnableBracketedPaste
        )?;

        if should_enable_keyboard_enhancement(supports_keyboard_enhancement())
            && execute!(
                out,
                PushKeyboardEnhancementFlags(requested_keyboard_enhancement_flags())
            )
            .is_ok()
        {
            keyboard_enhancement_enabled.store(true, Ordering::SeqCst);
        }

        Ok(Self {
            keyboard_enhancement_enabled,
        })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut out = stdout();
        if take_keyboard_enhancement(&self.keyboard_enhancement_enabled) {
            let _ = execute!(out, PopKeyboardEnhancementFlags);
        }
        let _ = execute!(
            out,
            Show,
            ResetColor,
            DisableFocusChange,
            DisableMouseCapture,
            DisableBracketedPaste,
            LeaveAlternateScreen
        );
        let _ = disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyboardEnhancementFlags;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn test_requested_keyboard_enhancement_flags_disambiguate_modified_keys() {
        let flags = requested_keyboard_enhancement_flags();
        assert!(flags.contains(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES));
        assert!(flags.contains(KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES));
        assert!(!flags.contains(KeyboardEnhancementFlags::REPORT_EVENT_TYPES));
    }

    #[test]
    fn test_keyboard_enhancement_query_failure_falls_back() {
        assert!(should_enable_keyboard_enhancement(Ok(true)));
        assert!(!should_enable_keyboard_enhancement(Ok(false)));
        assert!(!should_enable_keyboard_enhancement(Err(
            std::io::Error::other("unsupported")
        )));
    }

    #[test]
    fn test_keyboard_enhancement_cleanup_is_claimed_once() {
        let state = AtomicBool::new(true);
        assert!(take_keyboard_enhancement(&state));
        assert!(!take_keyboard_enhancement(&state));
    }
}
