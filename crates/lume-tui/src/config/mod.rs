//! Configuration parsing, keymap resolution, and settings schema for Lume.

pub mod keymap;
pub mod toml_parser;

use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use lume_core::{Editor, RopeBuffer};

pub use keymap::{
    format_key_sequence, parse_key_sequence, KeyCombo, KeymapContext, KeymapPreset, KeymapTable,
    LookupResult,
};
use toml_parser::{parse_table_header, parse_toml_document, TomlDocument, TomlEntry, TomlValue};

/// Errors produced while reading, parsing, or validating `config.toml`.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Failed to read or create the configuration file on disk.
    #[error("failed to access config `{}`: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Syntax error encountered while parsing TOML content.
    #[error("config parse error on line {line}: {message}")]
    Parse { line: usize, message: String },
    /// Semantic error (unknown section/key, out-of-range value, invalid keybinding or action).
    #[error("{}", format_validation_error(*line, message))]
    Validation {
        line: Option<usize>,
        message: String,
    },
}

fn format_validation_error(line: Option<usize>, message: &str) -> String {
    match line {
        Some(line_no) => format!("config validation error on line {line_no}: {message}"),
        None => format!("config validation error: {message}"),
    }
}

/// Editor behavior settings under `[editor]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorSettings {
    pub tab_size: usize,
    pub smart_indent: bool,
    pub smart_unindent: bool,
    pub auto_pairs: bool,
    pub modal: bool,
    pub line_numbers: bool,
    pub auto_save: bool,
    pub auto_save_debounce_ms: u64,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            tab_size: 4,
            smart_indent: true,
            smart_unindent: true,
            auto_pairs: true,
            modal: false,
            line_numbers: true,
            auto_save: false,
            auto_save_debounce_ms: 1000,
        }
    }
}

/// File explorer settings under `[explorer]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExplorerSettings {
    pub visible_on_startup: bool,
    pub width: usize,
}

impl Default for ExplorerSettings {
    fn default() -> Self {
        Self {
            visible_on_startup: true,
            width: 26,
        }
    }
}

/// Complete validated application configuration.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppConfig {
    pub editor: EditorSettings,
    pub explorer: ExplorerSettings,
    pub preset: KeymapPreset,
    pub keymaps: KeymapTable,
}

/// Result of loading configuration during application startup.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoadedConfig {
    pub config: AppConfig,
    pub path: Option<PathBuf>,
    pub warning: Option<String>,
}

/// Resolves the target configuration file path using CLI options and XDG conventions.
///
/// Resolution order when `no_config` is `false`:
/// 1. Explicit `--config <PATH>` (`cli_path`)
/// 2. `$LUME_CONFIG` (if non-empty)
/// 3. `$XDG_CONFIG_HOME/lume/config.toml` (if non-empty)
/// 4. `$HOME/.config/lume/config.toml` (or `%APPDATA%/lume/config.toml` on Windows)
pub fn resolve_config_path(cli_path: Option<&Path>, no_config: bool) -> Option<PathBuf> {
    if no_config {
        return None;
    }
    if let Some(path) = cli_path {
        return Some(path.to_path_buf());
    }
    if let Some(env_path) = env::var_os("LUME_CONFIG") {
        if !env_path.is_empty() {
            return Some(PathBuf::from(env_path));
        }
    }
    if let Some(xdg) = env::var_os("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join("lume").join("config.toml"));
        }
    }
    if let Some(home) = env::var_os("HOME") {
        if !home.is_empty() {
            return Some(
                PathBuf::from(home)
                    .join(".config")
                    .join("lume")
                    .join("config.toml"),
            );
        }
    }
    #[cfg(windows)]
    if let Some(appdata) = env::var_os("APPDATA") {
        if !appdata.is_empty() {
            return Some(PathBuf::from(appdata).join("lume").join("config.toml"));
        }
    }
    None
}

/// Returns a commented, valid default `config.toml` template for scaffolding and `--print-default-config`.
pub fn default_config_toml() -> String {
    let mut config = String::from(
        r#"# Lume configuration file
# Default location: ~/.config/lume/config.toml (override with $LUME_CONFIG or --config)
# Reload at runtime via Command Palette -> "Config: Reload Configuration".

[editor]
tab_size = 4
smart_indent = true
smart_unindent = true
auto_pairs = true
modal = false
line_numbers = true
auto_save = false
auto_save_debounce_ms = 1000

[explorer]
visible_on_startup = true
width = 26

[keymaps]
# The tables below define the active shortcuts.
# Use "none" or "" to disable a key in its context, including global fallback.
# Multi-key sequences use spaces (e.g. "Space f f" or "j k").
format = "explicit"
preset = "default"
"#,
    );
    let defaults = KeymapTable::default();
    for ctx in KeymapContext::all() {
        let _ = writeln!(config, "\n[keymaps.{}]", ctx.section_suffix());
        for (seq, action) in defaults.entries_for(ctx) {
            write_binding(&mut config, seq, action);
        }
    }
    config
}

fn write_binding(output: &mut String, seq: &[KeyCombo], action: &str) {
    let key = format_key_sequence(seq)
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let _ = writeln!(output, "\"{key}\" = \"{action}\"");
}

fn is_unbind(action: &str) -> bool {
    action.is_empty()
        || action.eq_ignore_ascii_case("none")
        || action.eq_ignore_ascii_case("unbind")
}

fn materialize_keymaps(content: &str, doc: &TomlDocument, keymaps: &KeymapTable) -> String {
    let newline = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut additions = BTreeMap::new();
    let mut inactive_lines = HashSet::new();
    additions.insert(
        String::from("keymaps"),
        String::from("format = \"explicit\"\n"),
    );
    for ctx in KeymapContext::all() {
        let section = format!("keymaps.{}", ctx.section_suffix());
        let mut existing = HashSet::new();
        if let Some(table) = doc.get(&section) {
            for (key, entry) in table {
                let seq = parse_key_sequence(key).expect("validated key sequence");
                let action = expect_str(entry, key).expect("validated action").trim();
                // Preserve bindings suppressed by later legacy overrides as comments.
                if !is_unbind(action)
                    && !matches!(keymaps.lookup(ctx, &seq.0), LookupResult::Matched(_))
                {
                    inactive_lines.insert(entry.line);
                }
                existing.insert(seq.0);
            }
        }
        let mut bindings = String::new();
        for (seq, action) in keymaps.entries_for(ctx) {
            if !existing.contains(seq) {
                write_binding(&mut bindings, seq, action);
            }
        }
        if !bindings.is_empty() {
            additions.insert(section, bindings);
        }
    }

    let mut migrated = String::new();
    for (idx, line) in content.split_inclusive('\n').enumerate() {
        if inactive_lines.contains(&(idx + 1)) {
            migrated.push_str("# ");
        }
        migrated.push_str(line);
        let header = if idx == 0 {
            line.trim_start_matches('\u{feff}')
        } else {
            line
        };
        if let Some(section) = header
            .trim_matches([' ', '\t', '\r', '\n'])
            .strip_prefix('[')
            .and_then(|header| parse_table_header(header, idx + 1).ok())
        {
            if let Some(bindings) = additions.remove(&section) {
                if !line.ends_with('\n') {
                    migrated.push_str(newline);
                }
                migrated.push_str(&bindings.replace('\n', newline));
            }
        }
    }
    // Add any tables that were absent from the original file, in context order.
    for section in std::iter::once(String::from("keymaps"))
        .chain(KeymapContext::all().map(|ctx| format!("keymaps.{}", ctx.section_suffix())))
    {
        if let Some(bindings) = additions.remove(&section) {
            if !migrated.is_empty() && !migrated.ends_with(newline) {
                migrated.push_str(newline);
            }
            let _ = write!(migrated, "{newline}[{section}]{newline}");
            migrated.push_str(&bindings.replace('\n', newline));
        }
    }
    migrated
}

impl AppConfig {
    /// Parses and validates an [`AppConfig`] from a TOML string.
    ///
    /// Action IDs in `[keymaps.*]` sections are validated with `validator`.
    #[cfg(test)]
    pub fn from_toml_str(
        input: &str,
        validator: impl Fn(&str) -> bool,
    ) -> Result<Self, ConfigError> {
        Self::from_document(&parse_toml_document(input)?, validator).map(|(config, _)| config)
    }

    fn from_document(
        doc: &TomlDocument,
        validator: impl Fn(&str) -> bool,
    ) -> Result<(Self, bool), ConfigError> {
        // Reject top-level keys outside any table.
        if let Some(root) = doc.get("") {
            if let Some((key, entry)) = root.iter().next() {
                return Err(ConfigError::Validation {
                    line: Some(entry.line),
                    message: format!("unexpected top-level key `{key}` outside any [section]"),
                });
            }
        }

        // Reject unknown table headers before applying fields.
        for (section, table) in doc {
            if section.is_empty() {
                continue;
            }
            let valid_section = matches!(section.as_str(), "editor" | "explorer" | "keymaps")
                || section
                    .strip_prefix("keymaps.")
                    .and_then(KeymapContext::from_section_suffix)
                    .is_some();
            if !valid_section {
                let line = table.values().next().map(|entry| entry.line);
                return Err(ConfigError::Validation {
                    line,
                    message: format!("unknown config section `[{section}]`"),
                });
            }
        }

        // Determine preset first so keymaps and Vim default modal setting build on it.
        let mut preset = KeymapPreset::Default;
        let mut explicit_keymaps = false;
        if let Some(keymaps_table) = doc.get("keymaps") {
            for (key, entry) in keymaps_table {
                match key.as_str() {
                    "format" => {
                        if expect_str(entry, "keymaps.format")? != "explicit" {
                            return Err(ConfigError::Validation {
                                line: Some(entry.line),
                                message: "keymaps.format must be \"explicit\"".into(),
                            });
                        }
                        explicit_keymaps = true;
                    }
                    "preset" => {
                        let raw = expect_str(entry, "keymaps.preset")?;
                        preset =
                            KeymapPreset::parse(raw).ok_or_else(|| ConfigError::Validation {
                                line: Some(entry.line),
                                message: format!(
                                    "unknown keymaps.preset `{raw}` (expected `default`, `minimal`, or `vim`)"
                                ),
                            })?;
                    }
                    other => {
                        return Err(ConfigError::Validation {
                            line: Some(entry.line),
                            message: format!("unknown key `{other}` in [keymaps]"),
                        });
                    }
                }
            }
        }

        let mut editor = EditorSettings::default();
        let mut explicit_modal = false;
        if let Some(editor_table) = doc.get("editor") {
            for (key, entry) in editor_table {
                match key.as_str() {
                    "tab_size" => {
                        let val = expect_int(entry, "editor.tab_size")?;
                        if !(1..=16).contains(&val) {
                            return Err(ConfigError::Validation {
                                line: Some(entry.line),
                                message: format!("editor.tab_size must be in 1..=16, got {val}"),
                            });
                        }
                        editor.tab_size = val as usize;
                    }
                    "smart_indent" => {
                        editor.smart_indent = expect_bool(entry, "editor.smart_indent")?;
                    }
                    "smart_unindent" => {
                        editor.smart_unindent = expect_bool(entry, "editor.smart_unindent")?;
                    }
                    "auto_pairs" => {
                        editor.auto_pairs = expect_bool(entry, "editor.auto_pairs")?;
                    }
                    "modal" => {
                        editor.modal = expect_bool(entry, "editor.modal")?;
                        explicit_modal = true;
                    }
                    "line_numbers" => {
                        editor.line_numbers = expect_bool(entry, "editor.line_numbers")?;
                    }
                    "auto_save" => {
                        editor.auto_save = expect_bool(entry, "editor.auto_save")?;
                    }
                    "auto_save_debounce_ms" => {
                        let val = expect_int(entry, "editor.auto_save_debounce_ms")?;
                        if !(100..=60_000).contains(&val) {
                            return Err(ConfigError::Validation {
                                line: Some(entry.line),
                                message: format!(
                                    "editor.auto_save_debounce_ms must be in 100..=60000, got {val}"
                                ),
                            });
                        }
                        editor.auto_save_debounce_ms = val;
                    }
                    other => {
                        return Err(ConfigError::Validation {
                            line: Some(entry.line),
                            message: format!("unknown key `{other}` in [editor]"),
                        });
                    }
                }
            }
        }

        if preset == KeymapPreset::Vim && !explicit_modal {
            editor.modal = true;
        }

        let mut explorer = ExplorerSettings::default();
        if let Some(explorer_table) = doc.get("explorer") {
            for (key, entry) in explorer_table {
                match key.as_str() {
                    "visible_on_startup" => {
                        explorer.visible_on_startup =
                            expect_bool(entry, "explorer.visible_on_startup")?;
                    }
                    "width" => {
                        let val = expect_int(entry, "explorer.width")?;
                        if !(12..=80).contains(&val) {
                            return Err(ConfigError::Validation {
                                line: Some(entry.line),
                                message: format!("explorer.width must be in 12..=80, got {val}"),
                            });
                        }
                        explorer.width = val as usize;
                    }
                    other => {
                        return Err(ConfigError::Validation {
                            line: Some(entry.line),
                            message: format!("unknown key `{other}` in [explorer]"),
                        });
                    }
                }
            }
        }

        let mut keymaps = if explicit_keymaps {
            KeymapTable::empty()
        } else {
            KeymapTable::from_preset(preset)
        };
        for (section, table) in doc {
            let Some(suffix) = section.strip_prefix("keymaps.") else {
                continue;
            };
            let Some(ctx) = KeymapContext::from_section_suffix(suffix) else {
                continue;
            };
            let mut ordered_entries: Vec<(&String, &TomlEntry)> = table.iter().collect();
            ordered_entries.sort_by_key(|(_, entry)| entry.line);
            for (key_spec, entry) in ordered_entries {
                let raw_action = expect_str(entry, &format!("{section}.{key_spec}"))?;
                let seq =
                    parse_key_sequence(key_spec).map_err(|err| attach_line(err, entry.line))?;
                let local = !ctx.local_bindings().is_empty();
                if local && seq.0.len() != 1 {
                    return Err(ConfigError::Validation {
                        line: Some(entry.line),
                        message: format!("[{section}] only accepts single-key shortcuts"),
                    });
                }
                let action = raw_action.trim();
                if is_unbind(action) {
                    if explicit_keymaps {
                        keymaps.disable(ctx, seq);
                    } else {
                        keymaps.unbind(ctx, &seq.0);
                    }
                } else {
                    let valid = if local {
                        ctx.local_bindings().iter().any(|&(_, id)| id == action)
                            || (action == "field.paste" && ctx.supports_overlay_paste())
                    } else {
                        validator(action)
                    };
                    if !valid {
                        return Err(ConfigError::Validation {
                            line: Some(entry.line),
                            message: format!("unknown action `{action}` in [{section}]"),
                        });
                    }
                    keymaps.bind(ctx, seq, action);
                }
            }
        }

        Ok((
            Self {
                editor,
                explorer,
                preset,
                keymaps,
            },
            explicit_keymaps,
        ))
    }

    /// Loads and validates configuration from an explicit file path on disk.
    pub fn load_from_path(
        path: &Path,
        validator: impl Fn(&str) -> bool,
    ) -> Result<Self, ConfigError> {
        let io_error = |source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        };
        let content = std::fs::read_to_string(path).map_err(io_error)?;
        let doc = parse_toml_document(&content)?;
        let (config, explicit) = Self::from_document(&doc, &validator)?;
        if explicit {
            return Ok(config);
        }

        let migrated = materialize_keymaps(&content, &doc, &config.keymaps);
        let (config, _) = Self::from_document(&parse_toml_document(&migrated)?, validator)?;
        let current = std::fs::read_to_string(path).map_err(io_error)?;
        if current != migrated {
            if current != content {
                return Err(io_error(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "configuration changed while migrating keymaps",
                )));
            }
            Editor::write_buffer_atomically(path, &RopeBuffer::from(migrated.as_str()))
                .map_err(io_error)?;
        }
        Ok(config)
    }

    /// Loads configuration at application startup using CLI flags and XDG/HOME defaults.
    ///
    /// Explicit `--config <PATH>` errors are fatal (`Err`). Errors in the auto-discovered
    /// default config fall back to [`AppConfig::default`] with [`LoadedConfig::warning`] set.
    pub fn load_startup(
        cli_path: Option<&Path>,
        no_config: bool,
        validator: impl Fn(&str) -> bool,
    ) -> Result<LoadedConfig, ConfigError> {
        let default_path = if cli_path.is_none() && !no_config {
            resolve_config_path(None, false)
        } else {
            None
        };
        Self::load_startup_with_paths(cli_path, default_path.as_deref(), no_config, validator)
    }

    pub(crate) fn load_startup_with_paths(
        cli_path: Option<&Path>,
        default_path: Option<&Path>,
        no_config: bool,
        validator: impl Fn(&str) -> bool,
    ) -> Result<LoadedConfig, ConfigError> {
        if no_config {
            return Ok(LoadedConfig::default());
        }

        if let Some(explicit) = cli_path {
            let config = Self::load_from_path(explicit, validator)?;
            return Ok(LoadedConfig {
                config,
                path: Some(explicit.to_path_buf()),
                warning: None,
            });
        }

        let Some(default_file) = default_path else {
            return Ok(LoadedConfig::default());
        };

        if !default_file.exists() {
            return Ok(LoadedConfig {
                config: AppConfig::default(),
                path: Some(default_file.to_path_buf()),
                warning: None,
            });
        }

        match Self::load_from_path(default_file, validator) {
            Ok(config) => Ok(LoadedConfig {
                config,
                path: Some(default_file.to_path_buf()),
                warning: None,
            }),
            Err(err) => Ok(LoadedConfig {
                config: AppConfig::default(),
                path: Some(default_file.to_path_buf()),
                warning: Some(format!("Config error: {err} (using defaults)")),
            }),
        }
    }
}

fn expect_bool(entry: &TomlEntry, key: &str) -> Result<bool, ConfigError> {
    match entry.value {
        TomlValue::Bool(v) => Ok(v),
        _ => Err(ConfigError::Validation {
            line: Some(entry.line),
            message: format!("expected boolean for `{key}`"),
        }),
    }
}

fn expect_int(entry: &TomlEntry, key: &str) -> Result<u64, ConfigError> {
    match entry.value {
        TomlValue::Integer(v) => Ok(v),
        _ => Err(ConfigError::Validation {
            line: Some(entry.line),
            message: format!("expected integer for `{key}`"),
        }),
    }
}

fn expect_str<'a>(entry: &'a TomlEntry, key: &str) -> Result<&'a str, ConfigError> {
    match &entry.value {
        TomlValue::Str(s) => Ok(s.as_str()),
        _ => Err(ConfigError::Validation {
            line: Some(entry.line),
            message: format!("expected string for `{key}`"),
        }),
    }
}

fn attach_line(err: ConfigError, line_no: usize) -> ConfigError {
    match err {
        ConfigError::Validation {
            line: None,
            message,
        } => ConfigError::Validation {
            line: Some(line_no),
            message,
        },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_validator(action: &str) -> bool {
        matches!(
            action,
            "app.quit"
                | "view.command_palette"
                | "file.quick_open"
                | "file.save"
                | "view.toggle_explorer"
                | "view.split_vertical"
                | "view.which_key"
                | "vim.normal_mode"
        )
    }

    #[test]
    fn minimal_preset_removes_counterintuitive_default_bindings() {
        let cfg = AppConfig::from_toml_str(
            r#"
[keymaps]
preset = "minimal"

[keymaps.global]
"Space f" = "file.quick_open"
"#,
            test_validator,
        )
        .unwrap();

        let ctrl_w = parse_key_sequence("Ctrl+W").unwrap().0;
        let ctrl_q = parse_key_sequence("Ctrl+Q").unwrap().0;
        let space_f = parse_key_sequence("Space f").unwrap().0;

        assert_eq!(
            cfg.keymaps.lookup(KeymapContext::Global, &ctrl_w),
            LookupResult::None
        );
        assert_eq!(
            cfg.keymaps.lookup(KeymapContext::Global, &ctrl_q),
            LookupResult::Matched("app.quit")
        );
        assert_eq!(
            cfg.keymaps.lookup(KeymapContext::Global, &space_f),
            LookupResult::Matched("file.quick_open")
        );
    }

    #[test]
    fn default_preset_allows_explicit_unbind_with_none_or_empty() {
        let cfg = AppConfig::from_toml_str(
            r#"
[keymaps.global]
"Ctrl+W" = "none"
"Ctrl+B" = ""
"#,
            test_validator,
        )
        .unwrap();

        let ctrl_w = parse_key_sequence("Ctrl+W").unwrap().0;
        let ctrl_b = parse_key_sequence("Ctrl+B").unwrap().0;
        assert_eq!(
            cfg.keymaps.lookup(KeymapContext::Global, &ctrl_w),
            LookupResult::None
        );
        assert_eq!(
            cfg.keymaps.lookup(KeymapContext::Global, &ctrl_b),
            LookupResult::None
        );
    }

    #[test]
    fn invalid_default_config_falls_back_with_warning_but_explicit_missing_errors() {
        let temp = crate::tests::TestDir::new("lume_cfg_fallback");
        let bad_path = temp.path().join("config.toml");
        std::fs::write(&bad_path, "[editor]\ntab_size = 0\n").unwrap();

        let loaded =
            AppConfig::load_startup_with_paths(None, Some(&bad_path), false, test_validator)
                .unwrap();
        assert_eq!(loaded.config, AppConfig::default());
        assert!(loaded.warning.is_some());

        let missing = temp.path().join("missing.toml");
        let err = AppConfig::load_startup_with_paths(Some(&missing), None, false, test_validator)
            .unwrap_err();
        assert!(matches!(err, ConfigError::Io { .. }));
    }

    #[test]
    fn default_config_template_round_trips_every_binding() {
        let config =
            AppConfig::from_toml_str(&default_config_toml(), crate::commands::is_valid_action_id)
                .unwrap();
        assert_eq!(config, AppConfig::default());
    }

    #[test]
    fn vim_preset_enables_modal_by_default_unless_explicitly_disabled() {
        let cfg =
            AppConfig::from_toml_str("[keymaps]\npreset = \"vim\"\n", test_validator).unwrap();
        assert!(cfg.editor.modal);
        assert_eq!(cfg.preset, KeymapPreset::Vim);

        let overridden = AppConfig::from_toml_str(
            "[editor]\nmodal = false\n[keymaps]\npreset = \"vim\"\n",
            test_validator,
        )
        .unwrap();
        assert!(!overridden.editor.modal);
    }

    #[test]
    fn unbind_removes_same_action_compatibility_aliases_without_touching_other_actions() {
        let cfg = AppConfig::from_toml_str(
            r#"
[keymaps.global]
"Ctrl+K W" = "none"
"Alt+P" = "none"
"Ctrl+C" = "none"
"Ctrl+P" = "none"
"#,
            |_| true,
        )
        .unwrap();

        let ctrl_k_w_lower = parse_key_sequence("Ctrl+K w").unwrap().0;
        let ctrl_k_w_upper = parse_key_sequence("Ctrl+K W").unwrap().0;
        let alt_shift_p = parse_key_sequence("Alt+Shift+P").unwrap().0;
        let ctrl_shift_c = parse_key_sequence("Ctrl+Shift+C").unwrap().0;
        let ctrl_p = parse_key_sequence("Ctrl+P").unwrap().0;
        let ctrl_shift_p = parse_key_sequence("Ctrl+Shift+P").unwrap().0;

        assert_eq!(
            cfg.keymaps.lookup(KeymapContext::Global, &ctrl_k_w_upper),
            LookupResult::None
        );
        assert_eq!(
            cfg.keymaps.lookup(KeymapContext::Global, &ctrl_k_w_lower),
            LookupResult::None
        );
        assert_eq!(
            cfg.keymaps.lookup(KeymapContext::Global, &alt_shift_p),
            LookupResult::None
        );
        assert_eq!(
            cfg.keymaps.lookup(KeymapContext::Global, &ctrl_shift_c),
            LookupResult::None
        );
        assert_eq!(
            cfg.keymaps.lookup(KeymapContext::Global, &ctrl_p),
            LookupResult::None
        );
        assert_eq!(
            cfg.keymaps.lookup(KeymapContext::Global, &ctrl_shift_p),
            LookupResult::Matched("view.command_palette")
        );
    }

    #[test]
    fn keymap_overrides_apply_in_source_line_order() {
        let cfg = AppConfig::from_toml_str(
            r#"
[keymaps]
preset = "minimal"

[keymaps.global]
"Ctrl+p" = "file.save"
"Ctrl+P" = "view.command_palette"
"Space f" = "file.quick_open"
"Space" = "none"
"#,
            test_validator,
        )
        .unwrap();

        let ctrl_p = parse_key_sequence("Ctrl+P").unwrap().0;
        let space_f = parse_key_sequence("Space f").unwrap().0;
        assert_eq!(
            cfg.keymaps.lookup(KeymapContext::Global, &ctrl_p),
            LookupResult::Matched("view.command_palette")
        );
        assert_eq!(
            cfg.keymaps.lookup(KeymapContext::Global, &space_f),
            LookupResult::None
        );
    }
}
