use std::io::sink;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use lume_core::{Journal, TextBuffer};

use crate::app::{ActiveFocus, App};
use crate::args::CliTargets;
use crate::commands::{is_valid_action_id, COMMANDS};
use crate::config::{AppConfig, KeymapPreset, LoadedConfig};
use crate::input::AppEvent;
use crate::lsp::{
    DefinitionQuery, DefinitionReply, DefinitionTarget, LspClient, LspError, Position,
    PositionEncoding, ServerKind,
};
use crate::prompt::PromptAction;
use crate::tests::TestDir;

fn attach_test_lsp(app: &mut App) -> crossbeam_channel::Receiver<DefinitionQuery> {
    let (tx, rx) = crossbeam_channel::unbounded();
    app.set_lsp_client(LspClient::with_test_sender(tx));
    rx
}

fn deliver_reply(app: &mut App, reply: DefinitionReply) {
    app.handle_event(AppEvent::Definition(reply), &mut sink())
        .unwrap();
}

#[test]
fn definition_opens_or_reuses_tab_at_symbol() {
    let dir = TestDir::new("lume_def_open_reuse");
    let src_dir = dir.path().join("src");
    let inc_dir = dir.path().join("include/arq/aaa");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&inc_dir).unwrap();

    let source_file = src_dir.join("t.c");
    let header_file = inc_dir.join("arch.h");
    let source_text = "#include \"arq/aaa/arch.h\"\nstruct Arch item;\n";
    let header_text = "struct Arch;\n\nstruct Arch { int value; };\n";
    std::fs::write(&source_file, source_text).unwrap();
    std::fs::write(&header_file, header_text).unwrap();

    let mut app = App::open_targets(CliTargets {
        workspace: Some(dir.path().to_path_buf()),
        files: vec![source_file.clone()],
        no_config: true,
        ..CliTargets::default()
    })
    .unwrap();
    let rx = attach_test_lsp(&mut app);

    // Position cursor on `Arch` in `struct Arch item;`
    let use_byte = source_text.find("struct Arch").unwrap() + "struct ".len();
    app.editor.cursors_mut().set_single(use_byte);

    // 1. Jumping to a not-yet-open header opens a new tab at the definition byte.
    app.request_definition_at_cursor();
    assert_eq!(
        app.status_message.as_ref().unwrap().0,
        "Looking up definition…"
    );
    let q1 = rx.try_recv().expect("expected LSP query");
    assert_eq!(q1.source_path, source_file);
    assert_eq!(q1.source_byte, use_byte);

    let expected_symbol_byte = header_text.find("struct Arch {").unwrap() + "struct ".len();
    deliver_reply(
        &mut app,
        DefinitionReply {
            id: q1.id,
            result: Ok(vec![DefinitionTarget {
                path: header_file.clone(),
                encoding: PositionEncoding::Utf16,
                position: Position {
                    line: 2,
                    character: 7,
                },
            }]),
        },
    );
    assert_eq!(app.editor.tabs().len(), 2);
    assert_eq!(app.editor.file_path(), Some(header_file.as_path()));
    assert_eq!(app.editor.cursors().primary().head, expected_symbol_byte);
    assert_eq!(app.editor.cursors().cursors().len(), 1);

    // 2. Open split: pane 0 has source_file, pane 1 has header_file (with its own cursor at byte 0).
    //    Re-trigger from pane 0 -> reuses existing header tab (no duplicate), keeps pane 0 active,
    //    and preserves pane 1's cursor.
    let target_tab_idx = app.editor.active_tab_idx();
    app.editor.cursors_mut().set_single(0);
    app.execute_command("view.split_vertical");
    // Now split is active with pane 1 active on target_tab_idx (cursor 0).
    app.switch_active_pane(0);
    app.editor.switch_tab(0);
    app.editor.cursors_mut().set_single(use_byte);
    app.sync_active_pane();

    let source_pane = app.splits.active_pane;
    let tab_count_before_reply = app.editor.tabs().len();

    app.request_definition_at_cursor();
    let q2 = rx.try_recv().expect("expected second LSP query");
    deliver_reply(
        &mut app,
        DefinitionReply {
            id: q2.id,
            result: Ok(vec![DefinitionTarget {
                path: header_file.clone(),
                encoding: PositionEncoding::Utf16,
                position: Position {
                    line: 2,
                    character: 7,
                },
            }]),
        },
    );

    assert_eq!(app.editor.tabs().len(), tab_count_before_reply);
    assert_eq!(app.editor.active_tab_idx(), target_tab_idx);
    assert_eq!(app.editor.cursors().primary().head, expected_symbol_byte);
    assert_eq!(app.splits.active_pane, source_pane);
    // Other split pane kept its own cursor (0)
    assert_eq!(app.splits.panes[1].cursors.primary().head, 0);
}

#[test]
fn definition_ignores_stale_replies() {
    let dir = TestDir::new("lume_def_stale");
    let src = dir.path().join("main.c");
    let other = dir.path().join("other.c");
    let target = dir.path().join("def.h");
    std::fs::write(&src, "int foo = 1;\nint bar = foo;\n").unwrap();
    std::fs::write(&other, "int other = 2;\n").unwrap();
    std::fs::write(&target, "int def_sym = 3;\n").unwrap();

    let mut app = App::open_targets(CliTargets {
        workspace: Some(dir.path().to_path_buf()),
        files: vec![src.clone(), other.clone()],
        no_config: true,
        ..CliTargets::default()
    })
    .unwrap();
    let rx = attach_test_lsp(&mut app);

    let def_target = DefinitionTarget {
        path: target.clone(),
        encoding: PositionEncoding::Utf16,
        position: Position {
            line: 0,
            character: 4,
        },
    };

    // 1. Tab change invalidates reply
    app.editor.switch_tab(0);
    app.request_definition_at_cursor();
    let q = rx.try_recv().unwrap();
    app.editor.switch_tab(1);
    deliver_reply(
        &mut app,
        DefinitionReply {
            id: q.id,
            result: Ok(vec![def_target.clone()]),
        },
    );
    assert_eq!(app.editor.tabs().len(), 2);
    assert_eq!(app.editor.active_tab_idx(), 1);

    // 2. Cursor movement invalidates reply
    app.editor.switch_tab(0);
    app.editor.cursors_mut().set_single(0);
    app.request_definition_at_cursor();
    let q = rx.try_recv().unwrap();
    app.editor.cursors_mut().set_single(4);
    deliver_reply(
        &mut app,
        DefinitionReply {
            id: q.id,
            result: Ok(vec![def_target.clone()]),
        },
    );
    assert_eq!(app.editor.tabs().len(), 2);
    assert_eq!(app.editor.cursors().primary().head, 4);

    // 3. Buffer revision change invalidates reply
    app.request_definition_at_cursor();
    let q = rx.try_recv().unwrap();
    app.editor.insert_char(' ').unwrap();
    app.editor.delete_backwards().unwrap();
    assert_eq!(app.editor.cursors().primary().head, 4);
    deliver_reply(
        &mut app,
        DefinitionReply {
            id: q.id,
            result: Ok(vec![def_target.clone()]),
        },
    );
    assert_eq!(app.editor.tabs().len(), 2);

    // 4. Open prompt is NOT overwritten by late reply
    app.request_definition_at_cursor();
    let q = rx.try_recv().unwrap();
    app.execute_command("navigation.goto_line");
    assert_eq!(app.prompt.as_ref().unwrap().action, PromptAction::GoToLine);
    deliver_reply(
        &mut app,
        DefinitionReply {
            id: q.id,
            result: Ok(vec![def_target.clone()]),
        },
    );
    assert_eq!(app.prompt.as_ref().unwrap().action, PromptAction::GoToLine);
    assert_eq!(app.editor.tabs().len(), 2);
    app.prompt = None;

    // 5. Split change invalidates reply
    app.execute_command("view.split_vertical");
    app.request_definition_at_cursor();
    let q = rx.try_recv().unwrap();
    let other_pane = if app.splits.active_pane == 0 { 1 } else { 0 };
    app.switch_active_pane(other_pane);
    deliver_reply(
        &mut app,
        DefinitionReply {
            id: q.id,
            result: Ok(vec![def_target.clone()]),
        },
    );
    assert_eq!(app.editor.tabs().len(), 2);
    app.execute_command("view.close_split");

    // 6. Out-of-order older reply is ignored while newer request remains active
    app.editor.switch_tab(0);
    app.request_definition_at_cursor();
    let q_old = rx.try_recv().unwrap();
    app.request_definition_at_cursor();
    let q_new = rx.try_recv().unwrap();
    assert_ne!(q_old.id, q_new.id);

    // Old reply arrives first -> ignored
    deliver_reply(
        &mut app,
        DefinitionReply {
            id: q_old.id,
            result: Ok(vec![def_target.clone()]),
        },
    );
    assert_eq!(app.editor.tabs().len(), 2);

    // Newer reply arrives -> applied!
    deliver_reply(
        &mut app,
        DefinitionReply {
            id: q_new.id,
            result: Ok(vec![def_target]),
        },
    );
    assert_eq!(app.editor.tabs().len(), 3);
    assert_eq!(app.editor.file_path(), Some(target.as_path()));
}

#[test]
fn definition_destination_selection() {
    let dir = TestDir::new("lume_def_select");
    let src = dir.path().join("use.c");
    let h1 = dir.path().join("a.h");
    let h2 = dir.path().join("b.h");
    std::fs::write(&src, "extern void fn_sym(void);\n").unwrap();
    std::fs::write(&h1, "void fn_sym(void);\n").unwrap();
    std::fs::write(&h2, "\nvoid fn_sym(void) {}\n").unwrap();

    let mut app = App::open_targets(CliTargets {
        workspace: Some(dir.path().to_path_buf()),
        files: vec![src.clone()],
        no_config: true,
        ..CliTargets::default()
    })
    .unwrap();
    let rx = attach_test_lsp(&mut app);

    let targets = vec![
        DefinitionTarget {
            path: h1.clone(),
            encoding: PositionEncoding::Utf16,
            position: Position {
                line: 0,
                character: 5,
            },
        },
        DefinitionTarget {
            path: h2.clone(),
            encoding: PositionEncoding::Utf16,
            position: Position {
                line: 1,
                character: 5,
            },
        },
    ];

    // 1. Multiple targets open SelectDefinition prompt; Esc preserves origin
    app.request_definition_at_cursor();
    let q1 = rx.try_recv().unwrap();
    deliver_reply(
        &mut app,
        DefinitionReply {
            id: q1.id,
            result: Ok(targets.clone()),
        },
    );
    assert!(matches!(
        app.prompt.as_ref().map(|p| &p.action),
        Some(PromptAction::SelectDefinition { .. })
    ));
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .unwrap();
    assert!(app.prompt.is_none());
    assert_eq!(app.editor.file_path(), Some(src.as_path()));
    assert_eq!(app.editor.tabs().len(), 1);

    // 2. Down + Enter opens the second target at its symbol byte
    app.request_definition_at_cursor();
    let q2 = rx.try_recv().unwrap();
    deliver_reply(
        &mut app,
        DefinitionReply {
            id: q2.id,
            result: Ok(targets),
        },
    );
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert!(app.prompt.is_none());
    assert_eq!(app.editor.file_path(), Some(h2.as_path()));
    assert_eq!(app.editor.cursors().primary().head, "\nvoid ".len());

    // 3. Scrolling past 5 destinations renders the selected item in view
    app.editor.switch_tab(0);
    app.request_definition_at_cursor();
    let q3 = rx.try_recv().unwrap();
    let many_targets: Vec<DefinitionTarget> = (0..8)
        .map(|i| DefinitionTarget {
            path: dir.path().join(format!("target_{i}.h")),
            encoding: PositionEncoding::Utf16,
            position: Position {
                line: i,
                character: 0,
            },
        })
        .collect();
    deliver_reply(
        &mut app,
        DefinitionReply {
            id: q3.id,
            result: Ok(many_targets),
        },
    );
    for _ in 0..7 {
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
            .unwrap();
    }
    let mut rendered = Vec::new();
    app.render(&mut rendered).unwrap();
    let ansi = String::from_utf8_lossy(&rendered);
    assert!(
        ansi.contains("target_7.h:8"),
        "expected scrolled 8th destination to be visible in prompt render"
    );
}

#[test]
fn definition_failure_preserves_origin_and_can_retry() {
    let dir = TestDir::new("lume_def_failures");
    let src = dir.path().join("src.c");
    let ok_hdr = dir.path().join("ok.h");
    let missing_hdr = dir.path().join("removed.h");
    let journal_hdr = dir.path().join("journaled.h");
    std::fs::write(&src, "int call_me(void);\n").unwrap();
    std::fs::write(&ok_hdr, "int call_me(void) { return 0; }\n").unwrap();
    std::fs::write(&journal_hdr, "int old_line;\nint call_me(void);\n").unwrap();

    let mut app = App::open_targets(CliTargets {
        workspace: Some(dir.path().to_path_buf()),
        files: vec![src.clone()],
        no_config: true,
        ..CliTargets::default()
    })
    .unwrap();
    let rx = attach_test_lsp(&mut app);

    // 1. Missing clangd binary reports status and preserves origin
    app.request_definition_at_cursor();
    let q1 = rx.try_recv().unwrap();
    deliver_reply(
        &mut app,
        DefinitionReply {
            id: q1.id,
            result: Err(LspError::MissingExecutable(ServerKind::Clangd)),
        },
    );
    assert!(app.status_message.as_ref().unwrap().0.contains("clangd"));
    assert_eq!(app.editor.file_path(), Some(src.as_path()));

    // 2. Timeout reports status and preserves origin
    app.request_definition_at_cursor();
    let q2 = rx.try_recv().unwrap();
    deliver_reply(
        &mut app,
        DefinitionReply {
            id: q2.id,
            result: Err(LspError::Timeout),
        },
    );
    assert!(app
        .status_message
        .as_ref()
        .unwrap()
        .0
        .to_lowercase()
        .contains("timed out"));
    assert_eq!(app.editor.file_path(), Some(src.as_path()));

    // 2b. ContentModified cancellation asks to retry without changing location
    app.request_definition_at_cursor();
    let q2b = rx.try_recv().unwrap();
    deliver_reply(
        &mut app,
        DefinitionReply {
            id: q2b.id,
            result: Err(LspError::ContentModified),
        },
    );
    assert!(app
        .status_message
        .as_ref()
        .unwrap()
        .0
        .to_lowercase()
        .contains("retry"));
    assert_eq!(app.editor.file_path(), Some(src.as_path()));

    // 3. Removed target file does NOT create a file and preserves origin tab
    app.request_definition_at_cursor();
    let q3 = rx.try_recv().unwrap();
    deliver_reply(
        &mut app,
        DefinitionReply {
            id: q3.id,
            result: Ok(vec![DefinitionTarget {
                path: missing_hdr.clone(),
                encoding: PositionEncoding::Utf16,
                position: Position {
                    line: 0,
                    character: 0,
                },
            }]),
        },
    );
    assert!(!missing_hdr.exists());
    assert_eq!(app.editor.file_path(), Some(src.as_path()));
    assert_eq!(app.editor.tabs().len(), 1);

    // 4. Retry succeeds cleanly
    app.request_definition_at_cursor();
    let q4 = rx.try_recv().unwrap();
    deliver_reply(
        &mut app,
        DefinitionReply {
            id: q4.id,
            result: Ok(vec![DefinitionTarget {
                path: ok_hdr.clone(),
                encoding: PositionEncoding::Utf16,
                position: Position {
                    line: 0,
                    character: 4,
                },
            }]),
        },
    );
    assert_eq!(app.editor.file_path(), Some(ok_hdr.as_path()));
    assert_eq!(app.editor.cursors().primary().head, 4);

    // 5. Untitled tab and unsupported language do not spawn or send any query
    app.editor.new_tab();
    app.request_definition_at_cursor();
    assert!(rx.try_recv().is_err());

    let py_file = dir.path().join("script.py");
    std::fs::write(&py_file, "def f(): pass\n").unwrap();
    app.editor.open_or_load(&py_file).unwrap();
    app.request_definition_at_cursor();
    assert!(rx.try_recv().is_err());

    let rs_file = dir.path().join("lib.rs");
    std::fs::write(&rs_file, "fn f() {}\n").unwrap();
    app.editor.open_or_load(&rs_file).unwrap();
    app.request_definition_at_cursor();
    assert!(rx.try_recv().is_err());

    // 6. Journal recovery on definition target preserves recovery flow and prompts repeat jump
    Journal::record(&journal_hdr, "// recovered header\n\nint call_me(void);\n").unwrap();
    app.editor.switch_tab(0);
    app.request_definition_at_cursor();
    let q5 = rx.try_recv().unwrap();
    deliver_reply(
        &mut app,
        DefinitionReply {
            id: q5.id,
            result: Ok(vec![DefinitionTarget {
                path: journal_hdr.clone(),
                encoding: PositionEncoding::Utf16,
                position: Position {
                    line: 1,
                    character: 4,
                },
            }]),
        },
    );
    assert!(matches!(
        app.prompt.as_ref().map(|p| &p.action),
        Some(PromptAction::ConfirmRecoverJournal { .. })
    ));
    app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE))
        .unwrap();
    assert!(app
        .editor
        .buffer()
        .slice_to_cow(0, app.editor.buffer().len_bytes())
        .starts_with("// recovered header"));
    assert!(app
        .status_message
        .as_ref()
        .unwrap()
        .0
        .to_lowercase()
        .contains("repeat"));
    let _ = Journal::remove(&journal_hdr);
}

#[test]
fn definition_keymap_and_palette_dispatch() {
    let dir = TestDir::new("lume_def_keymap");
    let src = dir.path().join("main.c");
    std::fs::write(&src, "int main(void) { return 0; }\n").unwrap();

    // Command catalog entry
    let cmd = COMMANDS
        .iter()
        .find(|c| c.id == "navigation.goto_definition")
        .expect("navigation.goto_definition must be registered");
    assert_eq!(cmd.title, "Navigation: Go to Definition");
    assert_eq!(cmd.shortcut, "F12");

    // 1. Default preset: F12 triggers definition lookup
    let mut app = App::open_targets(CliTargets {
        workspace: Some(dir.path().to_path_buf()),
        files: vec![src.clone()],
        no_config: true,
        ..CliTargets::default()
    })
    .unwrap();
    let rx = attach_test_lsp(&mut app);
    app.handle_key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE))
        .unwrap();
    assert!(rx.try_recv().is_ok());

    // 2. Vim preset: F12 works AND `g d` still triggers navigation.goto_line
    let vim_cfg =
        AppConfig::from_toml_str("[keymaps]\npreset = \"vim\"\n", is_valid_action_id).unwrap();
    assert_eq!(vim_cfg.preset, KeymapPreset::Vim);
    let mut vim_app = App::open_targets_with_config(
        CliTargets {
            workspace: Some(dir.path().to_path_buf()),
            files: vec![src.clone()],
            ..CliTargets::default()
        },
        LoadedConfig {
            config: vim_cfg,
            path: None,
            warning: None,
        },
    )
    .unwrap();
    let vim_rx = attach_test_lsp(&mut vim_app);
    vim_app.focus = ActiveFocus::Editor;
    vim_app
        .handle_key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE))
        .unwrap();
    assert!(vim_rx.try_recv().is_ok());

    // `g d` in Normal mode preserves Go to Line
    vim_app
        .handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE))
        .unwrap();
    vim_app
        .handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(
        vim_app.prompt.as_ref().map(|p| &p.action),
        Some(&PromptAction::GoToLine)
    );
    assert!(vim_rx.try_recv().is_err());

    // 3. Minimal preset: F12 is NOT bound, accessible via Command Palette, and remappable
    let min_cfg =
        AppConfig::from_toml_str("[keymaps]\npreset = \"minimal\"\n", is_valid_action_id).unwrap();
    let mut min_app = App::open_targets_with_config(
        CliTargets {
            workspace: Some(dir.path().to_path_buf()),
            files: vec![src.clone()],
            ..CliTargets::default()
        },
        LoadedConfig {
            config: min_cfg,
            path: None,
            warning: None,
        },
    )
    .unwrap();
    let min_rx = attach_test_lsp(&mut min_app);
    min_app
        .handle_key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE))
        .unwrap();
    assert!(min_rx.try_recv().is_err());

    // Trigger via Command Palette in minimal preset
    min_app.open_command_palette();
    min_app.palette_query = "Go to Definition".to_string();
    min_app
        .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert!(min_rx.try_recv().is_ok());

    // Custom remap in config.toml works
    let remap_cfg = AppConfig::from_toml_str(
        "[keymaps]\npreset = \"minimal\"\n[keymaps.global]\n\"Ctrl+D\" = \"navigation.goto_definition\"\n",
        is_valid_action_id,
    )
    .unwrap();
    let mut remap_app = App::open_targets_with_config(
        CliTargets {
            workspace: Some(dir.path().to_path_buf()),
            files: vec![PathBuf::from(&src)],
            ..CliTargets::default()
        },
        LoadedConfig {
            config: remap_cfg,
            path: None,
            warning: None,
        },
    )
    .unwrap();
    let remap_rx = attach_test_lsp(&mut remap_app);
    remap_app
        .handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(remap_rx.try_recv().is_ok());
}
