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
