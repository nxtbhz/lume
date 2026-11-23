use crate::app::App;
use crate::args::CliTargets;
use crate::commands::is_valid_action_id;
use crate::config::{AppConfig, KeymapPreset, LoadedConfig};
use crate::tests::TestDir;

#[test]
fn app_new_stays_isolated_from_env_config() {
    let dir = TestDir::new("lume_cfg_isolation");
    let cfg_path = dir.path().join("custom.toml");
    std::fs::write(&cfg_path, "[editor]\ntab_size = 2\nmodal = true\n").unwrap();

    let app = App::new(None).unwrap();
    assert_eq!(app.editor.config().tab_size, 4);
    assert!(!app.modal_enabled);

    // Explicit --config via open_targets loads the file as expected.
    let explicit_app = App::open_targets(CliTargets {
        config_path: Some(cfg_path),
        ..CliTargets::default()
    })
    .unwrap();
    assert_eq!(explicit_app.editor.config().tab_size, 2);
    assert!(explicit_app.modal_enabled);
}

#[test]
fn app_applies_editor_and_explorer_settings_from_loaded_config() {
    let cfg = AppConfig::from_toml_str(
        r#"
[editor]
tab_size = 2
smart_indent = false
auto_pairs = false
modal = true
line_numbers = false
auto_save = true
auto_save_debounce_ms = 500

[explorer]
visible_on_startup = false
width = 34

[keymaps]
preset = "vim"
"#,
        is_valid_action_id,
    )
    .unwrap();

    let app = App::open_targets_with_config(
        CliTargets::default(),
        LoadedConfig {
            config: cfg,
            path: None,
            warning: None,
        },
    )
    .unwrap();

    assert_eq!(app.editor.config().tab_size, 2);
    assert!(!app.editor.config().smart_indent);
    assert!(!app.editor.config().auto_pairs);
    assert!(app.modal_enabled);
    assert!(!app.show_line_numbers);
    assert!(app.editor.autosave_controller().config.enabled);
    assert_eq!(
        app.editor.autosave_controller().config.debounce.as_millis(),
        500
    );
    assert!(!app.explorer.is_visible);
    assert_eq!(app.explorer.width, 34);
    assert_eq!(app.config.preset, KeymapPreset::Vim);
}

#[test]
fn config_open_scaffolds_missing_file_and_config_reload_keeps_valid_config_on_error() {
    let dir = TestDir::new("lume_cfg_reload");
    let cfg_path = dir.path().join("lume/config.toml");

    let mut app = App::open_targets_with_config(
        CliTargets::default(),
        LoadedConfig {
            config: AppConfig::default(),
            path: Some(cfg_path.clone()),
            warning: None,
        },
    )
    .unwrap();

    app.execute_command("config.open");
    assert!(cfg_path.exists());
    assert_eq!(app.editor.file_path(), Some(cfg_path.as_path()));

    std::fs::write(&cfg_path, "[editor]\ntab_size = 999\n").unwrap();
    app.execute_command("config.reload");
    assert_eq!(app.editor.config().tab_size, 4);
    assert!(app
        .status_message
        .as_ref()
        .unwrap()
        .0
        .contains("Config error"));
}

#[test]
fn custom_minimal_keymap_disables_ctrl_w_and_uses_custom_chord() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use lume_core::TextBuffer;

    let invalid = AppConfig::from_toml_str(
        r#"
[keymaps]
preset = "minimal"

[keymaps.global]
"Space s" = "file.split_noop_check"
"#,
        is_valid_action_id,
    );
    assert!(invalid.is_err());

    let cfg = AppConfig::from_toml_str(
        r#"
[editor]
tab_size = 2

[keymaps]
preset = "minimal"

[keymaps.global]
"Alt+n" = "file.new_tab"
"Space v" = "view.split_vertical"
"#,
        is_valid_action_id,
    )
    .unwrap();

    let mut app = App::open_targets_with_config(
        CliTargets::default(),
        LoadedConfig {
            config: cfg,
            path: None,
            warning: None,
        },
    )
    .unwrap();

    // Ctrl+W does not close tab under minimal preset (and does not prompt/modify tabs)
    app.editor.new_tab();
    assert_eq!(app.editor.tabs().len(), 2);
    app.handle_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(app.editor.tabs().len(), 2);

    // Alt+n opens a new tab
    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::ALT))
        .unwrap();
    assert_eq!(app.editor.tabs().len(), 3);

    // Esc cancels a pending chord cleanly
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .unwrap();
    assert!(!app.pending_keys.is_empty());
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .unwrap();
    assert!(app.pending_keys.is_empty());
    assert!(!app.splits.is_split);

    // Chord "Space v" splits vertically
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .unwrap();
    assert!(!app.pending_keys.is_empty());
    app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE))
        .unwrap();
    assert!(app.splits.is_split);
    assert!(app.pending_keys.is_empty());

    // Tab uses configured tab_size = 2
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.editor.buffer().slice_to_cow(0, 2), "  ");
}

#[test]
fn insert_mode_multi_key_sequence_replays_pending_chars_on_mismatch() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let cfg = AppConfig::from_toml_str(
        r#"
[editor]
modal = true

[keymaps]
preset = "vim"

[keymaps.insert]
"j k" = "vim.normal_mode"
"#,
        is_valid_action_id,
    )
    .unwrap();

    let mut app = App::open_targets_with_config(
        CliTargets::default(),
        LoadedConfig {
            config: cfg,
            path: None,
            warning: None,
        },
    )
    .unwrap();

    // Enter Insert mode, type "jx" -> replays 'j' and inserts 'x', staying in Insert
    app.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.vim_state.mode, crate::vim::VimMode::Insert);
    assert_eq!(app.editor.buffer().rope().to_string(), "jx");

    // Type "jk" -> returns to Normal without inserting "jk"
    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.vim_state.mode, crate::vim::VimMode::Normal);
    assert_eq!(app.editor.buffer().rope().to_string(), "jx");
}

#[test]
fn which_key_and_palette_reflect_active_keymap_preset_and_overrides() {
    let cfg = AppConfig::from_toml_str(
        r#"
[keymaps]
preset = "minimal"

[keymaps.global]
"Space f" = "file.quick_open"
"#,
        crate::commands::is_valid_action_id,
    )
    .unwrap();

    let mut app = App::open_targets_with_config(
        CliTargets::default(),
        LoadedConfig {
            config: cfg,
            path: None,
            warning: None,
        },
    )
    .unwrap();

    assert_eq!(app.shortcut_label_for_command("file.quick_open"), "Space f");
    assert_eq!(app.shortcut_label_for_command("file.close_tab"), "");

    let lines = crate::render::palette::build_which_key_lines(&app);
    let joined = lines.join("\n");
    assert!(joined.contains("Space f"));
    assert!(!joined.contains("Ctrl+W"));

    // Verify render_command_palette renders "Space f" in the ANSI buffer
    app.open_command_palette();
    let mut out = Vec::new();
    crate::render::palette::render_command_palette(&app, &mut out).unwrap();
    let rendered = String::from_utf8_lossy(&out);
    assert!(rendered.contains("Space f"));

    // Verify status bar reflects active keymap hints and pending key sequence
    app.palette_open = false;
    app.status_message = None;
    app.terminal_width = 160;
    let mut status_out = Vec::new();
    crate::render::render_status_bar(&app, &mut status_out, 0).unwrap();
    let status_text = String::from_utf8_lossy(&status_out);
    assert!(status_text.contains("Space f: Quick Open"));
    assert!(!status_text.contains("Ctrl+S: Save"));

    app.pending_keys
        .push(crate::config::KeyCombo::parse("Space").unwrap());
    let mut pending_out = Vec::new();
    crate::render::render_status_bar(&app, &mut pending_out, 0).unwrap();
    let pending_text = String::from_utf8_lossy(&pending_out);
    assert!(pending_text.contains("Keys: Space ..."));
}

#[test]
fn pending_chord_completion_and_mismatch_do_not_leave_stale_status_message() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let cfg = AppConfig::from_toml_str(
        r#"
[editor]
modal = true

[keymaps]
preset = "vim"

[keymaps.insert]
"j k" = "vim.normal_mode"
"#,
        is_valid_action_id,
    )
    .unwrap();

    let mut app = App::open_targets_with_config(
        CliTargets::default(),
        LoadedConfig {
            config: cfg,
            path: None,
            warning: None,
        },
    )
    .unwrap();
    app.status_message = None;

    app.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))
        .unwrap();
    assert!(
        app.status_message
            .as_ref()
            .is_none_or(|(m, _)| !m.contains("Key sequence")),
        "stale status after mismatch: {:?}",
        app.status_message
    );

    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))
        .unwrap();
    assert!(
        app.status_message
            .as_ref()
            .is_none_or(|(m, _)| !m.contains("Key sequence")),
        "stale status after completion: {:?}",
        app.status_message
    );
}

#[test]
fn non_char_pending_key_replays_on_mismatch_in_insert_mode() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let cfg = AppConfig::from_toml_str(
        r#"
[editor]
tab_size = 2
modal = true

[keymaps]
preset = "vim"

[keymaps.insert]
"Tab k" = "vim.normal_mode"
"#,
        is_valid_action_id,
    )
    .unwrap();

    let mut app = App::open_targets_with_config(
        CliTargets::default(),
        LoadedConfig {
            config: cfg,
            path: None,
            warning: None,
        },
    )
    .unwrap();

    app.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.vim_state.mode, crate::vim::VimMode::Insert);
    assert_eq!(app.editor.buffer().rope().to_string(), "  x");
}
