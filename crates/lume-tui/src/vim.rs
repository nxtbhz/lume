#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VimMode {
    #[default]
    Normal,
    Insert,
    Visual,
}

impl VimMode {
    pub fn name(&self) -> &'static str {
        match self {
            VimMode::Normal => "NORMAL",
            VimMode::Insert => "INSERT",
            VimMode::Visual => "VISUAL",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct VimState {
    pub mode: VimMode,
    pub pending_operator: Option<char>,
    pub yank_register: Option<(String, bool)>, // (text, is_linewise)
}

impl VimState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_mode(&mut self, mode: VimMode) {
        self.mode = mode;
        self.pending_operator = None;
    }

    pub fn set_pending(&mut self, op: char) {
        self.pending_operator = Some(op);
    }

    pub fn clear_pending(&mut self) -> Option<char> {
        self.pending_operator.take()
    }

    pub fn yank(&mut self, text: String, linewise: bool) {
        self.yank_register = Some((text, linewise));
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn test_vim_state_transitions_and_pending() {
        let mut state = VimState::new();
        assert_eq!(state.mode, VimMode::Normal);
        assert_eq!(state.mode.name(), "NORMAL");

        state.set_pending('g');
        assert_eq!(state.pending_operator, Some('g'));
        assert_eq!(state.clear_pending(), Some('g'));
        assert_eq!(state.pending_operator, None);

        state.set_mode(VimMode::Insert);
        assert_eq!(state.mode, VimMode::Insert);
        assert_eq!(state.mode.name(), "INSERT");

        state.set_mode(VimMode::Visual);
        assert_eq!(state.mode, VimMode::Visual);
        assert_eq!(state.mode.name(), "VISUAL");
    }

    #[test]
    fn test_vim_yank_register() {
        let mut state = VimState::new();
        assert!(state.yank_register.is_none());

        state.yank("hello\n".to_string(), true);
        assert_eq!(state.yank_register, Some(("hello\n".to_string(), true)));

        state.yank("inline".to_string(), false);
        assert_eq!(state.yank_register, Some(("inline".to_string(), false)));
    }
}
