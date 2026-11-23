use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::{ActiveFocus, App};
use crate::config::{KeyCombo, KeymapContext, LookupResult};
use crate::vim::VimMode;
use lume_core::TextBuffer;

/// Outcome of routing a single [`KeyEvent`] through the active [`crate::config::KeymapTable`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchOutcome {
    /// A bound action was executed (or a pending chord was cancelled).
    Executed,
    /// The key extended or started a multi-key chord prefix.
    Pending,
    /// No binding or prefix matched in the active or global contexts.
    Unmatched,
}

enum ResolvedLookup {
    Matched(String),
    Prefix,
    None,
}

impl App {
    pub fn handle_key(&mut self, key: KeyEvent) -> std::io::Result<()> {
        if key.kind == KeyEventKind::Release {
            return Ok(());
        }

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);

        // 1. If Which-Key overlay is open, any key dismisses it
        if self.which_key_open {
            self.which_key_open = false;
            if key.code == KeyCode::Esc || key.code == KeyCode::F(1) {
                return Ok(());
            }
        }

        if ctrl && !alt && matches!(key.code, KeyCode::Char('v' | 'V')) && self.has_paste_overlay()
        {
            if let Some(text) = self.editor.clipboard().map(str::to_owned) {
                self.paste_into_overlay(&text);
            }
            return Ok(());
        }

        // 2. If an Input Prompt is open (Open File, Save As, Language Mode)
        if let Some(prompt) = self.prompt.take() {
            return self.handle_prompt_key(prompt, key);
        }

        // 3. If Search Bar is open (Find or Replace)
        if let Some(state) = self.search_state.take() {
            return self.handle_search_key(state, key);
        }

        // 3.5. If Quick Open is open, route keys to quick open
        if let Some(state) = self.quick_open.take() {
            return self.handle_quick_open_key(state, key);
        }

        // 4. If Command Palette is open, route keys to palette
        if self.palette_open {
            return self.handle_palette_key(key);
        }

        // 5. Unified keymap dispatch across Global + active context
        let active_ctx = self.active_keymap_context();
        if self.dispatch_keymap(active_ctx, key) == DispatchOutcome::Unmatched {
            match active_ctx {
                KeymapContext::Explorer => self.handle_explorer_fallback(key),
                KeymapContext::Editor | KeymapContext::Insert => {
                    self.handle_text_editing_fallback(key);
                }
                KeymapContext::Normal | KeymapContext::Visual | KeymapContext::Global => {}
            }
        }

        Ok(())
    }

    pub(crate) fn active_keymap_context(&self) -> KeymapContext {
        if self.focus == ActiveFocus::Explorer && self.explorer.is_visible {
            KeymapContext::Explorer
        } else if self.modal_enabled {
            match self.vim_state.mode {
                VimMode::Normal => KeymapContext::Normal,
                VimMode::Visual => KeymapContext::Visual,
                VimMode::Insert => KeymapContext::Insert,
            }
        } else {
            KeymapContext::Editor
        }
    }

    /// Dispatches `key` against `ctx` and [`KeymapContext::Global`], managing multi-key chords.
    pub fn dispatch_keymap(&mut self, ctx: KeymapContext, key: KeyEvent) -> DispatchOutcome {
        let combo = KeyCombo::from_key_event(key);

        if !self.pending_keys.is_empty()
            && combo.code == KeyCode::Esc
            && combo.modifiers == KeyModifiers::NONE
        {
            self.clear_pending_keys();
            self.set_status("Cancelled chord");
            return DispatchOutcome::Executed;
        }

        if self.pending_keys.is_empty() {
            return self.dispatch_single_combo(ctx, combo);
        }

        self.pending_keys.push(combo);
        match self.resolve_sequence(ctx, &self.pending_keys) {
            ResolvedLookup::Matched(action) => {
                self.clear_pending_keys();
                self.execute_command(&action);
                DispatchOutcome::Executed
            }
            ResolvedLookup::Prefix => {
                self.sync_pending_state(ctx);
                DispatchOutcome::Pending
            }
            ResolvedLookup::None => {
                self.pending_keys.pop();
                let prior = std::mem::take(&mut self.pending_keys);
                self.vim_state.clear_pending();

                let prior_has_modifiers = prior.iter().any(|k| {
                    k.modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                });
                if prior_has_modifiers {
                    self.set_status("Cancelled chord");
                    return DispatchOutcome::Executed;
                }

                if matches!(ctx, KeymapContext::Editor | KeymapContext::Insert) {
                    for prev in prior {
                        self.handle_text_editing_fallback(KeyEvent::new(prev.code, prev.modifiers));
                    }
                }

                // Re-evaluate the current key alone with an empty pending buffer.
                self.dispatch_single_combo(ctx, combo)
            }
        }
    }

    fn dispatch_single_combo(&mut self, ctx: KeymapContext, combo: KeyCombo) -> DispatchOutcome {
        match self.resolve_sequence(ctx, &[combo]) {
            ResolvedLookup::Matched(action) => {
                self.execute_command(&action);
                DispatchOutcome::Executed
            }
            ResolvedLookup::Prefix => {
                self.pending_keys.push(combo);
                self.sync_pending_state(ctx);
                DispatchOutcome::Pending
            }
            ResolvedLookup::None => DispatchOutcome::Unmatched,
        }
    }

    fn resolve_sequence(&self, ctx: KeymapContext, seq: &[KeyCombo]) -> ResolvedLookup {
        if ctx != KeymapContext::Global {
            match self.config.keymaps.lookup(ctx, seq) {
                LookupResult::Matched(action) => return ResolvedLookup::Matched(action.to_owned()),
                LookupResult::Prefix => return ResolvedLookup::Prefix,
                LookupResult::None => {}
            }
        }
        match self.config.keymaps.lookup(KeymapContext::Global, seq) {
            LookupResult::Matched(action) => ResolvedLookup::Matched(action.to_owned()),
            LookupResult::Prefix => ResolvedLookup::Prefix,
            LookupResult::None => ResolvedLookup::None,
        }
    }

    fn sync_pending_state(&mut self, ctx: KeymapContext) {
        if matches!(ctx, KeymapContext::Normal | KeymapContext::Visual)
            && self.pending_keys.len() == 1
        {
            if let KeyCombo {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::NONE,
            } = self.pending_keys[0]
            {
                self.vim_state.set_pending(c);
            } else {
                self.vim_state.clear_pending();
            }
        } else {
            self.vim_state.clear_pending();
        }
    }

    pub(crate) fn handle_text_editing_fallback(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        if ctrl || alt {
            return;
        }

        let tab_id = self.editor.active_tab().id;
        let cursor = *self.editor.cursors().primary();

        match key.code {
            KeyCode::Backspace => {
                let len_before = self.editor.buffer().len_bytes();
                let min_pos = cursor.min();
                if self.editor.delete_backwards().is_ok() {
                    let len_after = self.editor.buffer().len_bytes();
                    let deleted = len_before.saturating_sub(len_after);
                    if deleted > 0 {
                        let start = if cursor.is_selection() {
                            min_pos
                        } else {
                            min_pos.saturating_sub(deleted)
                        };
                        self.shift_syntax_for_delta(tab_id, start, deleted, 0);
                    }
                }
            }
            KeyCode::Delete => {
                let len_before = self.editor.buffer().len_bytes();
                let start = cursor.min();
                if self.editor.delete_forwards().is_ok() {
                    let len_after = self.editor.buffer().len_bytes();
                    let deleted = len_before.saturating_sub(len_after);
                    if deleted > 0 {
                        self.shift_syntax_for_delta(tab_id, start, deleted, 0);
                    }
                }
            }
            KeyCode::Enter => {
                self.insert_with_syntax_shift(|editor| editor.insert_newline().is_ok());
            }
            KeyCode::Tab => {
                let spaces = " ".repeat(self.editor.config().tab_size.max(1));
                self.insert_with_syntax_shift(|editor| editor.insert_str(&spaces).is_ok());
            }
            KeyCode::Char(c) => {
                self.insert_char_with_syntax(c);
            }
            _ => return,
        }

        self.adjust_viewport();
    }

    pub(crate) fn insert_char_with_syntax(&mut self, ch: char) {
        self.insert_with_syntax_shift(|editor| editor.insert_char(ch).is_ok());
    }

    fn insert_with_syntax_shift(&mut self, op: impl FnOnce(&mut lume_core::Editor) -> bool) {
        let tab_id = self.editor.active_tab().id;
        let cursor = *self.editor.cursors().primary();
        let len_before = self.editor.buffer().len_bytes();
        let start = cursor.min();
        let old_len = if cursor.is_selection() {
            cursor.max() - cursor.min()
        } else {
            0
        };
        if op(&mut self.editor) {
            let len_after = self.editor.buffer().len_bytes();
            let new_len =
                (len_after as isize - len_before as isize + old_len as isize).max(0) as usize;
            self.shift_syntax_for_delta(tab_id, start, old_len, new_len);
        }
    }
}
