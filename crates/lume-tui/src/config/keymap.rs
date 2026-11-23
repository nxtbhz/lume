//! Key representation, cross-terminal normalization, and key-sequence parsing.

use std::cmp::Ordering;
use std::fmt;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::ConfigError;

const RELEVANT_MODIFIERS: KeyModifiers = KeyModifiers::CONTROL
    .union(KeyModifiers::ALT)
    .union(KeyModifiers::SHIFT);

/// Canonical, `Copy` representation of a single key press with normalized modifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyCombo {
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
        let (norm_code, norm_mods) = normalize(code, modifiers, false);
        Ok(Self {
            code: norm_code,
            modifiers: norm_mods,
        })
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
    /// All supported keymap contexts in canonical order.
    pub const ALL: [Self; 6] = [
        Self::Global,
        Self::Editor,
        Self::Normal,
        Self::Visual,
        Self::Insert,
        Self::Explorer,
    ];

    /// Lowercase TOML section suffix for this context.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Editor => "editor",
            Self::Normal => "normal",
            Self::Visual => "visual",
            Self::Insert => "insert",
            Self::Explorer => "explorer",
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
        let ev_lower_ctrl_shift =
            KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL | KeyModifiers::SHIFT);
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
