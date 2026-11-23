//! Key representation, cross-terminal normalization, key-sequence parsing, and [`KeymapTable`].

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashSet};
use std::fmt;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::ConfigError;

const RELEVANT_MODIFIERS: KeyModifiers = KeyModifiers::CONTROL
    .union(KeyModifiers::ALT)
    .union(KeyModifiers::SHIFT);

/// Built-in keymap preset selected under `[keymaps] preset = "..."`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum KeymapPreset {
    /// Full IDE shortcuts compatible with Lume's classic bindings.
    #[default]
    Default,
    /// Only essential global bindings (`Ctrl+Q`, `Alt+P`, `F1`) plus basic navigation.
    Minimal,
    /// Modal Vim-first bindings with `Space` leader and `:` for Command Palette.
    Vim,
}

impl KeymapPreset {
    /// Parses a preset name (`"default"`, `"minimal"`, `"vim"`).
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "default" => Some(Self::Default),
            "minimal" => Some(Self::Minimal),
            "vim" => Some(Self::Vim),
            _ => None,
        }
    }

    /// Lowercase name for display and serialization.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Minimal => "minimal",
            Self::Vim => "vim",
        }
    }
}

/// Canonical, `Copy` representation of a single key press with normalized modifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyCombo {
    /// Constructs a canonical [`KeyCombo`] directly from a [`KeyCode`] and [`KeyModifiers`].
    pub fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        let (code, modifiers) = normalize(code, modifiers, false);
        Self { code, modifiers }
    }

    /// Converts a terminal [`KeyEvent`] into a canonical [`KeyCombo`].
    ///
    /// Normalizes cross-terminal differences such as `BackTab` vs `Shift+Tab`,
    /// `Char('P') + CONTROL` vs `Char('p') + CONTROL | SHIFT`, and redundant
    /// `SHIFT` on already-shifted symbols/uppercase letters.
    pub fn from_key_event(key: KeyEvent) -> Self {
        let (code, modifiers) = normalize(key.code, key.modifiers, true);
        Self { code, modifiers }
    }

    /// Parses a single key specification such as `"Ctrl+Shift+P"`, `"Alt+Right"`,
    /// `"Space"`, `"G"`, or `"Ctrl++"`.
    pub fn parse(spec: &str) -> Result<Self, ConfigError> {
        let spec = spec.trim();
        if spec.is_empty() {
            return Err(ConfigError::Validation {
                line: None,
                message: "empty key specification".to_string(),
            });
        }

        let (modifier_part, key_token) = split_key_spec(spec)?;
        let mut modifiers = KeyModifiers::NONE;

        if let Some(mod_str) = modifier_part {
            for part in mod_str.split('+') {
                if part.is_empty() {
                    return Err(ConfigError::Validation {
                        line: None,
                        message: format!("invalid modifier syntax in key `{spec}`"),
                    });
                }
                match part.to_ascii_lowercase().as_str() {
                    "ctrl" | "control" => modifiers |= KeyModifiers::CONTROL,
                    "alt" | "meta" | "opt" => modifiers |= KeyModifiers::ALT,
                    "shift" => modifiers |= KeyModifiers::SHIFT,
                    other => {
                        return Err(ConfigError::Validation {
                            line: None,
                            message: format!("unknown modifier `{other}` in key `{spec}`"),
                        });
                    }
                }
            }
        }

        let code = parse_key_code(key_token, spec)?;
        Ok(Self::new(code, modifiers))
    }

    fn sort_key(self) -> (u8, u8, u32) {
        let mod_bits = self.modifiers.bits();
        let (tag, val) = match self.code {
            KeyCode::Char(c) => (0u8, c as u32),
            KeyCode::F(n) => (1, u32::from(n)),
            KeyCode::Enter => (2, 0),
            KeyCode::Esc => (2, 1),
            KeyCode::Tab => (2, 2),
            KeyCode::BackTab => (2, 3),
            KeyCode::Backspace => (2, 4),
            KeyCode::Delete => (2, 5),
            KeyCode::Insert => (2, 6),
            KeyCode::Up => (2, 7),
            KeyCode::Down => (2, 8),
            KeyCode::Left => (2, 9),
            KeyCode::Right => (2, 10),
            KeyCode::Home => (2, 11),
            KeyCode::End => (2, 12),
            KeyCode::PageUp => (2, 13),
            KeyCode::PageDown => (2, 14),
            _ => (255, 0),
        };
        (mod_bits, tag, val)
    }
}

impl Ord for KeyCombo {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_key().cmp(&other.sort_key())
    }
}

impl PartialOrd for KeyCombo {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for KeyCombo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.modifiers.contains(KeyModifiers::CONTROL) {
            f.write_str("Ctrl+")?;
        }
        if self.modifiers.contains(KeyModifiers::ALT) {
            f.write_str("Alt+")?;
        }
        if self.modifiers.contains(KeyModifiers::SHIFT) {
            f.write_str("Shift+")?;
        }

        match self.code {
            KeyCode::Char(' ') => f.write_str("Space"),
            KeyCode::Char(c) => {
                if self
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                    && c.is_ascii_alphabetic()
                {
                    write!(f, "{}", c.to_ascii_uppercase())
                } else {
                    write!(f, "{c}")
                }
            }
            KeyCode::Enter => f.write_str("Enter"),
            KeyCode::Esc => f.write_str("Esc"),
            KeyCode::Tab => f.write_str("Tab"),
            KeyCode::BackTab => f.write_str("BackTab"),
            KeyCode::Backspace => f.write_str("Backspace"),
            KeyCode::Delete => f.write_str("Delete"),
            KeyCode::Insert => f.write_str("Insert"),
            KeyCode::Up => f.write_str("Up"),
            KeyCode::Down => f.write_str("Down"),
            KeyCode::Left => f.write_str("Left"),
            KeyCode::Right => f.write_str("Right"),
            KeyCode::Home => f.write_str("Home"),
            KeyCode::End => f.write_str("End"),
            KeyCode::PageUp => f.write_str("PageUp"),
            KeyCode::PageDown => f.write_str("PageDown"),
            KeyCode::F(n) => write!(f, "F{n}"),
            other => write!(f, "{other:?}"),
        }
    }
}

/// Ordered sequence of one or more key presses (a single shortcut or multi-key chord).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeySequence(pub Vec<KeyCombo>);

/// Input context in which a keybinding is active.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeymapContext {
    Global,
    Editor,
    Normal,
    Visual,
    Insert,
    Explorer,
}

impl KeymapContext {
    /// All supported keymap contexts in canonical priority order.
    pub const ALL: [Self; 6] = [
        Self::Global,
        Self::Editor,
        Self::Normal,
        Self::Visual,
        Self::Insert,
        Self::Explorer,
    ];

    /// Parses a context suffix from `[keymaps.<ctx>]`.
    pub fn from_section_suffix(suffix: &str) -> Option<Self> {
        match suffix {
            "global" => Some(Self::Global),
            "editor" => Some(Self::Editor),
            "normal" => Some(Self::Normal),
            "visual" => Some(Self::Visual),
            "insert" => Some(Self::Insert),
            "explorer" => Some(Self::Explorer),
            _ => None,
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Global => 0,
            Self::Editor => 1,
            Self::Normal => 2,
            Self::Visual => 3,
            Self::Insert => 4,
            Self::Explorer => 5,
        }
    }
}

/// Result of looking up a key sequence in a [`KeymapContext`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookupResult<'a> {
    /// Sequence exactly matches a bound action ID.
    Matched(&'a str),
    /// Sequence is a proper prefix of at least one bound chord.
    Prefix,
    /// Sequence matches neither a binding nor a chord prefix.
    None,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ContextBindings {
    bindings: BTreeMap<Vec<KeyCombo>, String>,
    prefixes: HashSet<Vec<KeyCombo>>,
    order: Vec<Vec<KeyCombo>>,
}

impl ContextBindings {
    fn bind(&mut self, seq: KeySequence, action: String) {
        if seq.0.is_empty() {
            return;
        }
        // Keep the prefix tree conflict-free within a single context: remove any
        // strictly shorter prefix or strictly longer extension of `seq`.
        self.bindings.retain(|existing, _| {
            if existing.as_slice() == seq.0.as_slice() {
                return true;
            }
            !(existing.starts_with(&seq.0) || seq.0.starts_with(existing))
        });
        self.order.retain(|existing| {
            existing.as_slice() == seq.0.as_slice()
                || !(existing.starts_with(&seq.0) || seq.0.starts_with(existing))
        });

        let is_new = !self.bindings.contains_key(&seq.0);
        if is_new {
            self.order.push(seq.0.clone());
        }
        self.bindings.insert(seq.0, action);
        self.rebuild_prefixes();
    }

    fn unbind(&mut self, seq: &[KeyCombo]) {
        if seq.is_empty() {
            return;
        }
        let before = self.bindings.len();
        self.bindings
            .retain(|existing, _| !existing.starts_with(seq));
        if self.bindings.len() != before {
            self.order.retain(|existing| !existing.starts_with(seq));
            self.rebuild_prefixes();
        }
    }

    fn rebuild_prefixes(&mut self) {
        self.prefixes.clear();
        for seq in self.bindings.keys() {
            for len in 1..seq.len() {
                self.prefixes.insert(seq[..len].to_vec());
            }
        }
    }

    fn lookup(&self, seq: &[KeyCombo]) -> LookupResult<'_> {
        if let Some(action) = self.bindings.get(seq) {
            return LookupResult::Matched(action.as_str());
        }
        if self.prefixes.contains(seq) {
            return LookupResult::Prefix;
        }
        LookupResult::None
    }

    fn entries(&self) -> Vec<(&[KeyCombo], &str)> {
        self.order
            .iter()
            .filter_map(|seq| {
                self.bindings
                    .get(seq.as_slice())
                    .map(|action| (seq.as_slice(), action.as_str()))
            })
            .collect()
    }
}

/// Multi-context keymap table supporting single shortcuts, chords, and unbinding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeymapTable {
    contexts: [ContextBindings; 6],
}

impl Default for KeymapTable {
    fn default() -> Self {
        Self::from_preset(KeymapPreset::Default)
    }
}

impl KeymapTable {
    /// Creates an empty keymap table with no bindings in any context.
    pub fn empty() -> Self {
        Self {
            contexts: std::array::from_fn(|_| ContextBindings::default()),
        }
    }

    /// Builds a keymap table pre-populated for the requested [`KeymapPreset`].
    pub fn from_preset(preset: KeymapPreset) -> Self {
        let mut table = Self::empty();
        match preset {
            KeymapPreset::Default => populate_default_preset(&mut table),
            KeymapPreset::Minimal => populate_minimal_preset(&mut table),
            KeymapPreset::Vim => populate_vim_preset(&mut table),
        }
        table
    }

    /// Binds `seq` in `ctx` to `action`, replacing any conflicting binding/prefix in `ctx`.
    pub fn bind(&mut self, ctx: KeymapContext, seq: KeySequence, action: impl Into<String>) {
        self.contexts[ctx.index()].bind(seq, action.into());
    }

    /// Removes `seq` (and any chord starting with `seq`) from `ctx`.
    pub fn unbind(&mut self, ctx: KeymapContext, seq: &[KeyCombo]) {
        self.contexts[ctx.index()].unbind(seq);
    }

    /// Looks up `seq` in `ctx` without heap allocation.
    pub fn lookup(&self, ctx: KeymapContext, seq: &[KeyCombo]) -> LookupResult<'_> {
        self.contexts[ctx.index()].lookup(seq)
    }

    /// Returns the first formatted shortcut bound to `action` across contexts.
    pub fn primary_shortcut_for(&self, action: &str) -> Option<String> {
        for ctx in KeymapContext::ALL {
            for (seq, bound_action) in self.entries_for(ctx) {
                if bound_action == action {
                    return Some(format_key_sequence(seq));
                }
            }
        }
        None
    }

    /// Returns all distinct formatted shortcuts bound to `action` across contexts.
    pub fn shortcuts_for(&self, action: &str) -> Vec<String> {
        let mut out = Vec::new();
        for ctx in KeymapContext::ALL {
            for (seq, bound_action) in self.entries_for(ctx) {
                if bound_action == action {
                    let formatted = format_key_sequence(seq);
                    if !out.contains(&formatted) {
                        out.push(formatted);
                    }
                }
            }
        }
        out
    }

    /// Lists all active `(sequence, action_id)` pairs in `ctx` in deterministic display order.
    pub fn entries_for(&self, ctx: KeymapContext) -> Vec<(&[KeyCombo], &str)> {
        self.contexts[ctx.index()].entries()
    }

    fn bind_one(&mut self, ctx: KeymapContext, code: KeyCode, mods: KeyModifiers, action: &str) {
        self.bind(ctx, KeySequence(vec![KeyCombo::new(code, mods)]), action);
    }

    fn bind_two(
        &mut self,
        ctx: KeymapContext,
        first: (KeyCode, KeyModifiers),
        second: (KeyCode, KeyModifiers),
        action: &str,
    ) {
        self.bind(
            ctx,
            KeySequence(vec![
                KeyCombo::new(first.0, first.1),
                KeyCombo::new(second.0, second.1),
            ]),
            action,
        );
    }
}

fn populate_basic_editor_navigation(table: &mut KeymapTable, ctx: KeymapContext) {
    const NONE: KeyModifiers = KeyModifiers::NONE;
    const SHIFT: KeyModifiers = KeyModifiers::SHIFT;
    const CTRL: KeyModifiers = KeyModifiers::CONTROL;
    const CTRL_SHIFT: KeyModifiers = KeyModifiers::CONTROL.union(KeyModifiers::SHIFT);

    table.bind_one(ctx, KeyCode::Left, NONE, "motion.left");
    table.bind_one(ctx, KeyCode::Right, NONE, "motion.right");
    table.bind_one(ctx, KeyCode::Up, NONE, "motion.up");
    table.bind_one(ctx, KeyCode::Down, NONE, "motion.down");
    table.bind_one(ctx, KeyCode::Left, SHIFT, "motion.select_left");
    table.bind_one(ctx, KeyCode::Right, SHIFT, "motion.select_right");
    table.bind_one(ctx, KeyCode::Up, SHIFT, "motion.select_up");
    table.bind_one(ctx, KeyCode::Down, SHIFT, "motion.select_down");
    table.bind_one(ctx, KeyCode::Left, CTRL, "motion.word_left");
    table.bind_one(ctx, KeyCode::Right, CTRL, "motion.word_right");
    table.bind_one(ctx, KeyCode::Left, CTRL_SHIFT, "motion.select_word_left");
    table.bind_one(ctx, KeyCode::Right, CTRL_SHIFT, "motion.select_word_right");
    table.bind_one(ctx, KeyCode::Home, NONE, "motion.home");
    table.bind_one(ctx, KeyCode::End, NONE, "motion.end");
    table.bind_one(ctx, KeyCode::Home, SHIFT, "motion.select_home");
    table.bind_one(ctx, KeyCode::End, SHIFT, "motion.select_end");
    table.bind_one(ctx, KeyCode::PageUp, NONE, "motion.page_up");
    table.bind_one(ctx, KeyCode::PageDown, NONE, "motion.page_down");
    table.bind_one(ctx, KeyCode::PageUp, SHIFT, "motion.select_page_up");
    table.bind_one(ctx, KeyCode::PageDown, SHIFT, "motion.select_page_down");
}

fn populate_basic_explorer_navigation(table: &mut KeymapTable) {
    const NONE: KeyModifiers = KeyModifiers::NONE;
    let ctx = KeymapContext::Explorer;

    table.bind_one(ctx, KeyCode::Up, NONE, "explorer.move_up");
    table.bind_one(ctx, KeyCode::Down, NONE, "explorer.move_down");
    table.bind_one(ctx, KeyCode::PageUp, NONE, "explorer.page_up");
    table.bind_one(ctx, KeyCode::PageDown, NONE, "explorer.page_down");
    table.bind_one(ctx, KeyCode::Left, NONE, "explorer.collapse");
    table.bind_one(ctx, KeyCode::Right, NONE, "explorer.expand");
    table.bind_one(ctx, KeyCode::Enter, NONE, "explorer.activate");
    table.bind_one(ctx, KeyCode::Char(' '), NONE, "explorer.activate");
    table.bind_one(ctx, KeyCode::Tab, NONE, "explorer.focus_editor");
    table.bind_one(ctx, KeyCode::Esc, NONE, "explorer.focus_editor");
}

fn populate_explorer_actions(table: &mut KeymapTable) {
    const NONE: KeyModifiers = KeyModifiers::NONE;
    let ctx = KeymapContext::Explorer;

    table.bind_one(ctx, KeyCode::Char('a'), NONE, "explorer.new_file");
    table.bind_one(ctx, KeyCode::Char('n'), NONE, "explorer.new_folder");
    table.bind_one(ctx, KeyCode::F(2), NONE, "explorer.rename");
    table.bind_one(ctx, KeyCode::Char('m'), NONE, "explorer.rename");
    table.bind_one(ctx, KeyCode::Char('d'), NONE, "explorer.delete_folder");
    table.bind_one(ctx, KeyCode::Delete, NONE, "explorer.delete_folder");
    table.bind_one(ctx, KeyCode::Char('r'), NONE, "explorer.refresh");
}

fn populate_vim_modes(table: &mut KeymapTable) {
    const NONE: KeyModifiers = KeyModifiers::NONE;

    // Insert mode
    table.bind_one(KeymapContext::Insert, KeyCode::Esc, NONE, "vim.normal_mode");
    populate_basic_editor_navigation(table, KeymapContext::Insert);

    // Normal mode
    let norm = KeymapContext::Normal;
    table.bind_one(norm, KeyCode::Esc, NONE, "vim.normal_mode");
    table.bind_one(norm, KeyCode::Char('h'), NONE, "motion.left");
    table.bind_one(norm, KeyCode::Left, NONE, "motion.left");
    table.bind_one(norm, KeyCode::Char('j'), NONE, "motion.down");
    table.bind_one(norm, KeyCode::Down, NONE, "motion.down");
    table.bind_one(norm, KeyCode::Char('k'), NONE, "motion.up");
    table.bind_one(norm, KeyCode::Up, NONE, "motion.up");
    table.bind_one(norm, KeyCode::Char('l'), NONE, "motion.right");
    table.bind_one(norm, KeyCode::Right, NONE, "motion.right");
    table.bind_one(norm, KeyCode::Char('w'), NONE, "motion.word_forward");
    table.bind_one(norm, KeyCode::Char('b'), NONE, "motion.word_left");
    table.bind_one(norm, KeyCode::Char('0'), NONE, "motion.home");
    table.bind_one(norm, KeyCode::Home, NONE, "motion.home");
    table.bind_one(norm, KeyCode::Char('$'), NONE, "motion.end");
    table.bind_one(norm, KeyCode::End, NONE, "motion.end");
    table.bind_two(
        norm,
        (KeyCode::Char('g'), NONE),
        (KeyCode::Char('g'), NONE),
        "motion.document_start",
    );
    table.bind_one(norm, KeyCode::Char('G'), NONE, "motion.document_end");
    table.bind_one(norm, KeyCode::PageUp, NONE, "motion.page_up");
    table.bind_one(norm, KeyCode::PageDown, NONE, "motion.page_down");
    table.bind_one(norm, KeyCode::Char('x'), NONE, "vim.delete_char");
    table.bind_two(
        norm,
        (KeyCode::Char('d'), NONE),
        (KeyCode::Char('d'), NONE),
        "vim.delete_line",
    );
    table.bind_two(
        norm,
        (KeyCode::Char('y'), NONE),
        (KeyCode::Char('y'), NONE),
        "vim.yank_line",
    );
    table.bind_one(norm, KeyCode::Char('p'), NONE, "vim.paste");
    table.bind_one(norm, KeyCode::Char('u'), NONE, "edit.undo");
    table.bind_one(norm, KeyCode::Char('i'), NONE, "vim.insert_mode");
    table.bind_one(norm, KeyCode::Char('a'), NONE, "vim.append_mode");
    table.bind_one(norm, KeyCode::Char('o'), NONE, "vim.open_line_below");
    table.bind_one(norm, KeyCode::Char('O'), NONE, "vim.open_line_above");
    table.bind_one(norm, KeyCode::Char('v'), NONE, "vim.visual_mode");

    // Visual mode
    let vis = KeymapContext::Visual;
    table.bind_one(vis, KeyCode::Esc, NONE, "vim.normal_mode");
    table.bind_one(vis, KeyCode::Char('v'), NONE, "vim.normal_mode");
    table.bind_one(vis, KeyCode::Char('h'), NONE, "motion.select_left");
    table.bind_one(vis, KeyCode::Left, NONE, "motion.select_left");
    table.bind_one(vis, KeyCode::Char('j'), NONE, "motion.select_down");
    table.bind_one(vis, KeyCode::Down, NONE, "motion.select_down");
    table.bind_one(vis, KeyCode::Char('k'), NONE, "motion.select_up");
    table.bind_one(vis, KeyCode::Up, NONE, "motion.select_up");
    table.bind_one(vis, KeyCode::Char('l'), NONE, "motion.select_right");
    table.bind_one(vis, KeyCode::Right, NONE, "motion.select_right");
    table.bind_one(vis, KeyCode::Char('w'), NONE, "motion.select_word_forward");
    table.bind_one(vis, KeyCode::Char('b'), NONE, "motion.select_word_left");
    table.bind_one(vis, KeyCode::Char('0'), NONE, "motion.select_home");
    table.bind_one(vis, KeyCode::Home, NONE, "motion.select_home");
    table.bind_one(vis, KeyCode::Char('$'), NONE, "motion.select_end");
    table.bind_one(vis, KeyCode::End, NONE, "motion.select_end");
    table.bind_two(
        vis,
        (KeyCode::Char('g'), NONE),
        (KeyCode::Char('g'), NONE),
        "motion.select_document_start",
    );
    table.bind_one(vis, KeyCode::Char('G'), NONE, "motion.select_document_end");
    table.bind_one(vis, KeyCode::Char('d'), NONE, "vim.visual_delete");
    table.bind_one(vis, KeyCode::Char('x'), NONE, "vim.visual_delete");
    table.bind_one(vis, KeyCode::Char('y'), NONE, "vim.visual_yank");
}

fn populate_minimal_preset(table: &mut KeymapTable) {
    const NONE: KeyModifiers = KeyModifiers::NONE;
    const CTRL: KeyModifiers = KeyModifiers::CONTROL;
    const ALT: KeyModifiers = KeyModifiers::ALT;
    const ALT_SHIFT: KeyModifiers = KeyModifiers::ALT.union(KeyModifiers::SHIFT);

    let g = KeymapContext::Global;
    table.bind_one(g, KeyCode::Char('q'), CTRL, "app.quit");
    table.bind_one(g, KeyCode::Char('p'), ALT, "view.command_palette");
    table.bind_one(g, KeyCode::Char('p'), ALT_SHIFT, "view.command_palette");
    table.bind_one(g, KeyCode::F(1), NONE, "view.which_key");

    populate_basic_editor_navigation(table, KeymapContext::Editor);
    table.bind_one(KeymapContext::Insert, KeyCode::Esc, NONE, "vim.normal_mode");
    populate_basic_editor_navigation(table, KeymapContext::Insert);
    populate_basic_explorer_navigation(table);
}

fn populate_vim_preset(table: &mut KeymapTable) {
    const NONE: KeyModifiers = KeyModifiers::NONE;
    const CTRL_ALT: KeyModifiers = KeyModifiers::CONTROL.union(KeyModifiers::ALT);

    populate_minimal_preset(table);
    table.bind_one(
        KeymapContext::Global,
        KeyCode::Char('v'),
        CTRL_ALT,
        "settings.toggle_modal",
    );
    populate_vim_modes(table);
    populate_explorer_actions(table);

    let norm = KeymapContext::Normal;
    table.bind_one(norm, KeyCode::Char(':'), NONE, "view.command_palette");
    table.bind_two(
        norm,
        (KeyCode::Char(' '), NONE),
        (KeyCode::Char('p'), NONE),
        "file.quick_open",
    );
    table.bind_two(
        norm,
        (KeyCode::Char(' '), NONE),
        (KeyCode::Char('e'), NONE),
        "view.toggle_explorer",
    );
    table.bind_two(
        norm,
        (KeyCode::Char(' '), NONE),
        (KeyCode::Char('w'), NONE),
        "file.save",
    );
    table.bind_two(
        norm,
        (KeyCode::Char(' '), NONE),
        (KeyCode::Char('q'), NONE),
        "app.quit",
    );
    table.bind_two(
        norm,
        (KeyCode::Char('g'), NONE),
        (KeyCode::Char('d'), NONE),
        "navigation.goto_line",
    );
}

fn populate_default_preset(table: &mut KeymapTable) {
    const NONE: KeyModifiers = KeyModifiers::NONE;
    const SHIFT: KeyModifiers = KeyModifiers::SHIFT;
    const CTRL: KeyModifiers = KeyModifiers::CONTROL;
    const ALT: KeyModifiers = KeyModifiers::ALT;
    const CTRL_SHIFT: KeyModifiers = KeyModifiers::CONTROL.union(KeyModifiers::SHIFT);
    const ALT_SHIFT: KeyModifiers = KeyModifiers::ALT.union(KeyModifiers::SHIFT);
    const CTRL_ALT: KeyModifiers = KeyModifiers::CONTROL.union(KeyModifiers::ALT);
    const CTRL_ALT_SHIFT: KeyModifiers = KeyModifiers::CONTROL
        .union(KeyModifiers::ALT)
        .union(KeyModifiers::SHIFT);

    let g = KeymapContext::Global;
    table.bind_one(g, KeyCode::Char('q'), CTRL, "app.quit");
    table.bind_one(g, KeyCode::Char('p'), CTRL_SHIFT, "view.command_palette");
    table.bind_one(g, KeyCode::Char('p'), ALT, "view.command_palette");
    table.bind_one(g, KeyCode::Char('p'), ALT_SHIFT, "view.command_palette");
    table.bind_one(g, KeyCode::Char('p'), CTRL, "file.quick_open");
    table.bind_one(g, KeyCode::Char('o'), CTRL, "file.open");
    table.bind_one(g, KeyCode::Char('s'), CTRL, "file.save");
    table.bind_one(g, KeyCode::Char('s'), CTRL_SHIFT, "file.save_as");
    table.bind_one(g, KeyCode::Char('b'), CTRL, "view.toggle_explorer");
    table.bind_one(g, KeyCode::F(2), NONE, "view.toggle_explorer");
    table.bind_one(g, KeyCode::Char('e'), CTRL, "view.toggle_explorer");
    table.bind_one(g, KeyCode::Char('t'), CTRL, "file.new_tab");
    table.bind_one(g, KeyCode::Char('w'), CTRL, "file.close_tab");
    table.bind_one(g, KeyCode::Tab, CTRL, "file.next_tab");
    table.bind_one(g, KeyCode::Right, ALT, "file.next_tab");
    table.bind_one(g, KeyCode::Tab, CTRL_SHIFT, "file.prev_tab");
    table.bind_one(g, KeyCode::Left, ALT, "file.prev_tab");
    table.bind_one(g, KeyCode::Char('a'), CTRL, "edit.select_all");
    table.bind_one(g, KeyCode::Char('a'), CTRL_SHIFT, "edit.select_all");
    table.bind_one(g, KeyCode::Char('z'), CTRL, "edit.undo");
    table.bind_one(g, KeyCode::Char('y'), CTRL, "edit.redo");
    table.bind_one(g, KeyCode::Char('r'), CTRL, "edit.redo");
    table.bind_one(g, KeyCode::Char('c'), CTRL, "edit.copy");
    table.bind_one(g, KeyCode::Char('c'), CTRL_SHIFT, "edit.copy");
    table.bind_one(g, KeyCode::Char('x'), CTRL, "edit.cut");
    table.bind_one(g, KeyCode::Char('x'), CTRL_SHIFT, "edit.cut");
    table.bind_one(g, KeyCode::Char('v'), CTRL, "edit.paste");
    table.bind_one(g, KeyCode::Char('v'), CTRL_SHIFT, "edit.paste");
    table.bind_one(g, KeyCode::Char('f'), CTRL, "edit.find");
    table.bind_one(g, KeyCode::Char('h'), CTRL, "edit.replace");
    table.bind_one(g, KeyCode::Char('m'), CTRL, "navigation.match_bracket");
    table.bind_one(g, KeyCode::Char('g'), CTRL, "navigation.goto_line");
    table.bind_one(g, KeyCode::Char('g'), CTRL_SHIFT, "navigation.goto_line");
    table.bind_one(g, KeyCode::Char('v'), CTRL_ALT, "settings.toggle_modal");
    table.bind_one(
        g,
        KeyCode::Char('v'),
        CTRL_ALT_SHIFT,
        "settings.toggle_modal",
    );
    table.bind_one(g, KeyCode::F(1), NONE, "view.which_key");
    table.bind_one(g, KeyCode::F(3), NONE, "search.find_next");
    table.bind_one(g, KeyCode::F(3), SHIFT, "search.find_prev");

    // Ctrl+K chords (bind canonical uppercase/symbol first for clean display labels)
    let ctrl_k = (KeyCode::Char('k'), CTRL);
    table.bind_two(
        g,
        ctrl_k,
        (KeyCode::Char('\\'), NONE),
        "view.split_vertical",
    );
    table.bind_two(g, ctrl_k, (KeyCode::Char('|'), NONE), "view.split_vertical");
    table.bind_two(
        g,
        ctrl_k,
        (KeyCode::Char('-'), NONE),
        "view.split_horizontal",
    );
    table.bind_two(
        g,
        ctrl_k,
        (KeyCode::Char('_'), NONE),
        "view.split_horizontal",
    );
    table.bind_two(
        g,
        ctrl_k,
        (KeyCode::Char('W'), NONE),
        "view.focus_next_split",
    );
    table.bind_two(
        g,
        ctrl_k,
        (KeyCode::Char('w'), NONE),
        "view.focus_next_split",
    );
    table.bind_two(g, ctrl_k, (KeyCode::Char('Q'), NONE), "view.close_split");
    table.bind_two(g, ctrl_k, (KeyCode::Char('q'), NONE), "view.close_split");
    table.bind_two(g, ctrl_k, (KeyCode::Char('M'), NONE), "language.set_mode");
    table.bind_two(g, ctrl_k, (KeyCode::Char('m'), NONE), "language.set_mode");

    populate_basic_editor_navigation(table, KeymapContext::Editor);
    populate_vim_modes(table);
    table.bind_one(
        KeymapContext::Normal,
        KeyCode::Char(':'),
        NONE,
        "navigation.goto_line",
    );
    populate_basic_explorer_navigation(table);
    populate_explorer_actions(table);
}

/// Parses a whitespace-separated sequence of key specifications (e.g., `"Space f f"`, `"Ctrl+K \\"`).
pub fn parse_key_sequence(spec: &str) -> Result<KeySequence, ConfigError> {
    let mut combos = Vec::new();
    for token in spec.split_whitespace() {
        combos.push(KeyCombo::parse(token)?);
    }
    if combos.is_empty() {
        return Err(ConfigError::Validation {
            line: None,
            message: "key sequence cannot be empty".to_string(),
        });
    }
    Ok(KeySequence(combos))
}

/// Formats a slice of [`KeyCombo`]s into a human-readable shortcut label.
pub fn format_key_sequence(seq: &[KeyCombo]) -> String {
    let mut out = String::new();
    for (idx, combo) in seq.iter().enumerate() {
        if idx > 0 {
            out.push(' ');
        }
        use std::fmt::Write as _;
        let _ = write!(out, "{combo}");
    }
    out
}

fn split_key_spec(spec: &str) -> Result<(Option<&str>, &str), ConfigError> {
    if spec == "+" {
        return Ok((None, "+"));
    }
    if let Some(prefix) = spec.strip_suffix("++") {
        if prefix.is_empty() {
            return Err(ConfigError::Validation {
                line: None,
                message: format!("missing modifier before `+` in key `{spec}`"),
            });
        }
        return Ok((Some(prefix), "+"));
    }
    if spec.ends_with('+') {
        return Err(ConfigError::Validation {
            line: None,
            message: format!("trailing `+` without key in `{spec}`"),
        });
    }
    match spec.rfind('+') {
        Some(idx) => {
            let prefix = &spec[..idx];
            if prefix.is_empty() {
                return Err(ConfigError::Validation {
                    line: None,
                    message: format!("leading `+` without modifier in `{spec}`"),
                });
            }
            Ok((Some(prefix), &spec[idx + 1..]))
        }
        None => Ok((None, spec)),
    }
}

fn parse_key_code(token: &str, full_spec: &str) -> Result<KeyCode, ConfigError> {
    if token.chars().count() == 1 {
        if let Some(ch) = token.chars().next() {
            return Ok(KeyCode::Char(ch));
        }
    }

    match token.to_ascii_lowercase().as_str() {
        "space" => Ok(KeyCode::Char(' ')),
        "enter" | "return" => Ok(KeyCode::Enter),
        "esc" | "escape" => Ok(KeyCode::Esc),
        "tab" => Ok(KeyCode::Tab),
        "backtab" => Ok(KeyCode::BackTab),
        "backspace" | "bs" => Ok(KeyCode::Backspace),
        "delete" | "del" => Ok(KeyCode::Delete),
        "insert" | "ins" => Ok(KeyCode::Insert),
        "up" => Ok(KeyCode::Up),
        "down" => Ok(KeyCode::Down),
        "left" => Ok(KeyCode::Left),
        "right" => Ok(KeyCode::Right),
        "home" => Ok(KeyCode::Home),
        "end" => Ok(KeyCode::End),
        "pageup" | "pgup" => Ok(KeyCode::PageUp),
        "pagedown" | "pgdn" => Ok(KeyCode::PageDown),
        lower => {
            if let Some(num_str) = lower.strip_prefix('f') {
                if let Ok(n) = num_str.parse::<u8>() {
                    if (1..=12).contains(&n) {
                        return Ok(KeyCode::F(n));
                    }
                }
            }
            Err(ConfigError::Validation {
                line: None,
                message: format!("unknown key `{token}` in `{full_spec}`"),
            })
        }
    }
}

fn normalize(
    code: KeyCode,
    modifiers: KeyModifiers,
    infer_shift_from_upper: bool,
) -> (KeyCode, KeyModifiers) {
    let mut mods = modifiers & RELEVANT_MODIFIERS;

    if code == KeyCode::BackTab {
        return (KeyCode::Tab, mods | KeyModifiers::SHIFT);
    }

    if let KeyCode::Char(c) = code {
        let has_ctrl_or_alt = mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        if c.is_ascii_alphabetic() {
            if has_ctrl_or_alt {
                if infer_shift_from_upper && c.is_ascii_uppercase() {
                    mods |= KeyModifiers::SHIFT;
                }
                return (KeyCode::Char(c.to_ascii_lowercase()), mods);
            }
            if c.is_ascii_uppercase() || mods.contains(KeyModifiers::SHIFT) {
                return (KeyCode::Char(c.to_ascii_uppercase()), KeyModifiers::NONE);
            }
            return (KeyCode::Char(c), KeyModifiers::NONE);
        }

        // Unmodified symbols already encode Shift in the character itself (e.g. '$', '_', '|', ':').
        if !has_ctrl_or_alt {
            mods.remove(KeyModifiers::SHIFT);
        }
        return (KeyCode::Char(c), mods);
    }

    (code, mods)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn parse_and_format_single_and_chord_sequences() {
        let seq = parse_key_sequence("Ctrl+K \\").unwrap();
        assert_eq!(seq.0.len(), 2);
        assert_eq!(format_key_sequence(&seq.0), "Ctrl+K \\");

        let leader = parse_key_sequence("Space f f").unwrap();
        assert_eq!(leader.0.len(), 3);
        assert_eq!(leader.0[0].code, KeyCode::Char(' '));
        assert_eq!(format_key_sequence(&leader.0), "Space f f");

        assert!(std::mem::size_of::<KeyCombo>() <= 16);
    }

    #[test]
    fn key_combo_normalizes_shift_uppercase_and_backtab_across_terminals() {
        let spec_ctrl_shift_p = KeyCombo::parse("Ctrl+Shift+P").unwrap();
        let ev_upper_ctrl = KeyEvent::new(KeyCode::Char('P'), KeyModifiers::CONTROL);
        let ev_lower_ctrl_shift = KeyEvent::new(
            KeyCode::Char('p'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        );
        assert_eq!(KeyCombo::from_key_event(ev_upper_ctrl), spec_ctrl_shift_p);
        assert_eq!(
            KeyCombo::from_key_event(ev_lower_ctrl_shift),
            spec_ctrl_shift_p
        );

        let spec_shift_tab = KeyCombo::parse("Shift+Tab").unwrap();
        assert_eq!(
            KeyCombo::from_key_event(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)),
            spec_shift_tab
        );
        assert_eq!(
            KeyCombo::from_key_event(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE)),
            spec_shift_tab
        );

        let spec_cap_g = KeyCombo::parse("G").unwrap();
        assert_eq!(
            KeyCombo::from_key_event(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT)),
            spec_cap_g
        );

        let spec_ctrl_plus = KeyCombo::parse("Ctrl++").unwrap();
        assert_eq!(
            KeyCombo::from_key_event(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::CONTROL)),
            spec_ctrl_plus
        );
    }

    #[test]
    fn parse_key_sequence_rejects_empty_and_unknown_keys() {
        assert!(parse_key_sequence("   ").is_err());
        assert!(parse_key_sequence("Ctrl+UnknownKey").is_err());
        assert!(parse_key_sequence("Ctrl+").is_err());
    }
}
