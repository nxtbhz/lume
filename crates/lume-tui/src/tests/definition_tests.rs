use std::io::sink;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
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

    // 5b. Active multi-key chord (e.g. Ctrl+K) invalidates reply and preserves pending chord
    app.editor.switch_tab(0);
    app.request_definition_at_cursor();
    let q_chord = rx.try_recv().unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(!app.pending_keys.is_empty());
    deliver_reply(
        &mut app,
        DefinitionReply {
            id: q_chord.id,
            result: Ok(vec![def_target.clone()]),
        },
    );
    assert_eq!(app.editor.tabs().len(), 2);
    assert_eq!(app.editor.file_path(), Some(src.as_path()));
    assert!(
        !app.pending_keys.is_empty(),
        "stale reply must not clear or corrupt an in-progress chord"
    );
    app.clear_pending_keys();

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

#[test]
fn definition_ctrl_click_uses_clicked_pane_and_text_position() {
    let dir = TestDir::new("lume_def_ctrl_click_pane");
    let left_file = dir.path().join("left.c");
    let right_file = dir.path().join("right.c");
    std::fs::write(&left_file, "int left_val = 0;\n").unwrap();
    // Line 0, Line 1, then Line 2 has tab (4 cols) + emoji (2 cols) + " struct Arch item;"
    // Visual columns on line 2:
    //   '\t' -> 0..4 (1 byte)
    //   '😀' -> 4..6 (4 bytes)
    //   ' '  -> 6..7 (1 byte)
    //   "struct " -> 7..14 (7 bytes)
    //   'A' of "Arch" -> col 14 (byte 13 from start of line 2)
    let right_content = "int l0;\nint l1;\n\t😀 struct Arch item;\n";
    std::fs::write(&right_file, right_content).unwrap();

    let mut app = App::open_targets(CliTargets {
        workspace: Some(dir.path().to_path_buf()),
        files: vec![left_file.clone(), right_file.clone()],
        no_config: true,
        ..CliTargets::default()
    })
    .unwrap();
    app.explorer.is_visible = false;
    app.terminal_width = 100;
    app.terminal_height = 24;
    let rx = attach_test_lsp(&mut app);

    // Tab 0 = left_file, Tab 1 = right_file.
    // Create vertical split: pane 0 shows left_file, pane 1 shows right_file scrolled to (row=2, col=4).
    app.editor.switch_tab(0);
    app.execute_command("view.split_vertical");
    // Now active_pane is 1; load right_file into pane 1 and scroll it
    app.editor.switch_tab(1);
    app.scroll_row = 2;
    app.scroll_col = 4;
    app.sync_active_pane();

    // Switch back to pane 0 so pane 1 (right_file) is INACTIVE
    app.switch_active_pane(0);
    assert_eq!(app.splits.active_pane, 0);
    assert_eq!(app.editor.active_tab_idx(), 0);

    let clicked_pane = 1usize;
    let clicked_file = right_file.clone();
    let clicked_tab_id = app.editor.tabs()[1].id;
    let expected_symbol_byte = right_content.find("Arch").unwrap();

    // Compute exact screen coordinates of 'A' in pane 1 (visual line 2, visual col 14, scroll=(2, 4))
    let text_left = app.text_area_left();
    let text_top = app.text_area_top();
    let area_w = (app.terminal_width as usize).saturating_sub(text_left);
    let text_h = app.text_area_height();
    let (_, _, p1_rect) = app.splits.layout(text_left, text_top, area_w, text_h);
    let p1 = p1_rect.unwrap();
    let p1_gutter = app.gutter_width_for_tab(1).min(p1.width);
    let click_row = p1.top; // line 2 with scroll_row = 2
    let click_col = p1.left + p1_gutter + (14 - 4); // visual col 14, scroll_col = 4

    let handled = app
        .handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: click_col as u16,
            row: click_row as u16,
            modifiers: KeyModifiers::CONTROL,
        })
        .unwrap();
    assert!(handled);

    let query = rx
        .try_recv()
        .expect("Ctrl+click on text in inactive pane must emit definition query");
    assert_eq!(query.source_path, clicked_file);
    assert_eq!(query.source_byte, expected_symbol_byte);
    assert_eq!(app.splits.active_pane, clicked_pane);
    assert_eq!(app.editor.active_tab().id, clicked_tab_id);
    assert!(!app.mouse_dragging);
}

#[test]
fn definition_ctrl_click_ignores_non_text_areas() {
    let dir = TestDir::new("lume_def_ctrl_click_ignore");
    let src = dir.path().join("sample.c");
    std::fs::write(&src, "int foo = 1;\nint bar = 2;\n").unwrap();

    let mut app = App::open_targets(CliTargets {
        workspace: Some(dir.path().to_path_buf()),
        files: vec![src.clone()],
        no_config: true,
        ..CliTargets::default()
    })
    .unwrap();
    app.explorer.is_visible = true;
    app.terminal_width = 100;
    app.terminal_height = 24;
    let rx = attach_test_lsp(&mut app);

    let ctrl_click = |app: &mut App, col: usize, row: usize| {
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col as u16,
            row: row as u16,
            modifiers: KeyModifiers::CONTROL,
        })
        .unwrap()
    };

    let exp_w = app.explorer_width();
    let text_left = app.text_area_left();
    let text_top = app.text_area_top();
    let gutter_w = app.gutter_width();

    // 1. Explorer area
    assert!(!ctrl_click(&mut app, 2, text_top));
    assert!(rx.try_recv().is_err());

    // 2. Gutter area
    assert!(!ctrl_click(&mut app, text_left, text_top));
    assert!(rx.try_recv().is_err());

    // 3. Space past end of line ("int foo = 1;" is 12 cols)
    assert!(!ctrl_click(&mut app, text_left + gutter_w + 25, text_top));
    assert!(rx.try_recv().is_err());

    // 4. Row past end of file
    assert!(!ctrl_click(
        &mut app,
        text_left + gutter_w + 1,
        text_top + 10
    ));
    assert!(rx.try_recv().is_err());

    // 5. Split separator
    app.execute_command("view.split_vertical");
    let area_w = (app.terminal_width as usize).saturating_sub(text_left);
    let (_, sep_rect, _) = app
        .splits
        .layout(text_left, text_top, area_w, app.text_area_height());
    let sep = sep_rect.unwrap();
    assert!(!ctrl_click(&mut app, sep.left, sep.top));
    assert!(rx.try_recv().is_err());
    app.execute_command("view.close_split");

    // 6. Overlays (palette, prompt, which-key, search)
    let valid_text_col = text_left + gutter_w + 4;
    app.palette_open = true;
    assert!(!ctrl_click(&mut app, valid_text_col, text_top));
    assert!(rx.try_recv().is_err());
    app.palette_open = false;

    app.execute_command("navigation.goto_line");
    assert!(!ctrl_click(&mut app, valid_text_col, text_top));
    assert!(rx.try_recv().is_err());
    app.prompt = None;

    app.which_key_open = true;
    assert!(!ctrl_click(&mut app, valid_text_col, text_top));
    assert!(rx.try_recv().is_err());
    app.which_key_open = false;

    app.execute_command("edit.find");
    assert!(!ctrl_click(&mut app, valid_text_col, text_top));
    assert!(rx.try_recv().is_err());
    app.search_state = None;

    // 7. Plain click, double-click word selection, and drag continue working without LSP queries
    assert!(exp_w > 0);
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: valid_text_col as u16,
        row: text_top as u16,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();
    assert!(app.mouse_dragging);
    assert_eq!(app.editor.cursors().primary().head, 4);
    assert!(rx.try_recv().is_err());

    // Drag right 3 columns selects "foo"
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: (valid_text_col + 3) as u16,
        row: text_top as u16,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();
    assert_eq!(app.editor.cursors().primary().range(), 4..7);

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: (valid_text_col + 3) as u16,
        row: text_top as u16,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();
    assert!(!app.mouse_dragging);

    // Double-click at same position selects word "foo"
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: valid_text_col as u16,
        row: text_top as u16,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();
    assert_eq!(app.editor.cursors().primary().range(), 4..7);
    assert!(rx.try_recv().is_err());
}

#[test]
#[ignore = "requires clangd installed on PATH"]
fn real_clangd_acceptance_matrix() {
    use std::time::Duration;

    fn wait_and_apply_reply(
        app: &mut App,
        reply_rx: &crossbeam_channel::Receiver<DefinitionReply>,
    ) {
        let reply = reply_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("timed out waiting for real clangd reply");
        deliver_reply(app, reply);
    }

    // --- 1. C project: struct forward decl vs full def + #include jump + tab reuse ---
    let c_dir = TestDir::new("lume_real_clangd_c");
    let c_src_dir = c_dir.path().join("src");
    let c_inc_dir = c_dir.path().join("include/arq/aaa");
    std::fs::create_dir_all(&c_src_dir).unwrap();
    std::fs::create_dir_all(&c_inc_dir).unwrap();
    std::fs::write(c_dir.path().join("compile_flags.txt"), "-Iinclude\n").unwrap();

    let t_c = c_src_dir.join("t.c");
    let arch_h = c_inc_dir.join("arch.h");
    let t_c_text = "#include \"arq/aaa/arch.h\"\n#include <stdio.h>\nint main(void) {\n    struct Arch item;\n    return item.value;\n}\n";
    let arch_h_text =
        "struct Arch;\n\n/* full definition below */\nstruct Arch {\n    int value;\n};\n";
    std::fs::write(&t_c, t_c_text).unwrap();
    std::fs::write(&arch_h, arch_h_text).unwrap();

    let (reply_tx, reply_rx) = crossbeam_channel::unbounded();
    let mut app = App::open_targets(CliTargets {
        workspace: Some(c_dir.path().to_path_buf()),
        files: vec![t_c.clone()],
        no_config: true,
        ..CliTargets::default()
    })
    .unwrap();
    app.explorer.is_visible = false;
    app.terminal_width = 100;
    app.terminal_height = 24;
    app.set_lsp_client(LspClient::new(move |reply| {
        let _ = reply_tx.send(reply);
    }));

    // F12 on `Arch` in `struct Arch item;` -> jumps to full definition on line 4 (0-based 3)
    let arch_use = t_c_text.find("struct Arch").unwrap() + "struct ".len();
    app.editor.cursors_mut().set_single(arch_use);
    app.handle_key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE))
        .unwrap();
    wait_and_apply_reply(&mut app, &reply_rx);
    let c_sym_path = app.editor.file_path().unwrap().to_path_buf();
    let c_sym_line = app.editor.cursor_line_col().0 + 1;
    println!(
        "[REAL CLANGD C symbol] -> {}:{}",
        c_sym_path.display(),
        c_sym_line
    );
    assert_eq!(
        c_sym_path.canonicalize().unwrap(),
        arch_h.canonicalize().unwrap()
    );
    assert_eq!(c_sym_line, 4);
    assert_eq!(app.editor.tabs().len(), 2);

    // Repeat F12 on `#include "arq/aaa/arch.h"` -> reuses existing header tab (still 2 tabs)
    app.editor.switch_tab(0);
    let inc_byte = t_c_text.find("arq/aaa/arch.h").unwrap() + 4;
    app.editor.cursors_mut().set_single(inc_byte);
    app.handle_key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE))
        .unwrap();
    wait_and_apply_reply(&mut app, &reply_rx);
    println!(
        "[REAL CLANGD C #include] -> {}:{} (tabs={})",
        app.editor.file_path().unwrap().display(),
        app.editor.cursor_line_col().0 + 1,
        app.editor.tabs().len()
    );
    assert_eq!(
        app.editor.file_path().unwrap().canonicalize().unwrap(),
        arch_h.canonicalize().unwrap()
    );
    assert_eq!(app.editor.tabs().len(), 2);

    // --- 2. Unsaved edits + split focus preservation ---
    // Edit arch.h in memory: insert 3 blank lines at top (moves `struct Arch {` from line 4 to line 7)
    app.editor.cursors_mut().set_single(0);
    app.editor.insert_str("\n\n\n").unwrap();
    assert!(app.editor.is_dirty());

    // Open vertical split: pane 1 holds arch.h with cursor at byte 0; pane 0 holds t.c
    app.editor.cursors_mut().set_single(0);
    app.execute_command("view.split_vertical");
    app.switch_active_pane(0);
    app.editor.switch_tab(0);
    app.editor.cursors_mut().set_single(arch_use);
    app.sync_active_pane();

    app.handle_key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE))
        .unwrap();
    wait_and_apply_reply(&mut app, &reply_rx);
    let moved_line = app.editor.cursor_line_col().0 + 1;
    println!(
        "[REAL CLANGD unsaved+split] -> {}:{} (tabs={}, active_pane={}, other_pane_cursor={})",
        app.editor.file_path().unwrap().display(),
        moved_line,
        app.editor.tabs().len(),
        app.splits.active_pane,
        app.splits.panes[1].cursors.primary().head
    );
    assert_eq!(moved_line, 7);
    assert_eq!(app.editor.tabs().len(), 2);
    assert_eq!(app.splits.active_pane, 0);
    assert_eq!(app.splits.panes[1].cursors.primary().head, 0);
    app.execute_command("view.close_split");

    // --- 3. External system header (<stdio.h>) without changing Explorer root ---
    let orig_explorer_root = app.explorer.root.clone();
    app.editor.switch_tab(0);
    let stdio_byte = t_c_text.find("stdio.h").unwrap() + 1;
    app.editor.cursors_mut().set_single(stdio_byte);
    app.handle_key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE))
        .unwrap();
    wait_and_apply_reply(&mut app, &reply_rx);
    let sys_path = app.editor.file_path().unwrap().to_path_buf();
    println!(
        "[REAL CLANGD system header] -> {} (explorer_root unchanged: {})",
        sys_path.display(),
        app.explorer.root == orig_explorer_root
    );
    assert!(sys_path.is_file());
    assert!(sys_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .contains("stdio"));
    assert_eq!(app.explorer.root, orig_explorer_root);

    // --- 4. Ctrl+click produces identical destination to F12 ---
    app.editor.switch_tab(0);
    app.scroll_row = 0;
    app.scroll_col = 0;
    // Line 3 (`    struct Arch item;`), `Arch` starts at col 11
    let click_col = app.text_area_left() + app.gutter_width() + 11;
    let click_row = app.text_area_top() + 3;
    let handled = app
        .handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: click_col as u16,
            row: click_row as u16,
            modifiers: KeyModifiers::CONTROL,
        })
        .unwrap();
    assert!(handled);
    wait_and_apply_reply(&mut app, &reply_rx);
    let ctrl_click_line = app.editor.cursor_line_col().0 + 1;
    println!(
        "[REAL CLANGD Ctrl+click] -> {}:{}",
        app.editor.file_path().unwrap().display(),
        ctrl_click_line
    );
    assert_eq!(
        app.editor.file_path().unwrap().canonicalize().unwrap(),
        arch_h.canonicalize().unwrap()
    );
    assert_eq!(ctrl_click_line, 7);

    // --- 5. C++ project with compile_commands.json, competing header dir, and cross-file method def ---
    let cpp_dir = TestDir::new("lume_real_clangd_cpp");
    let cpp_src = cpp_dir.path().join("src");
    let cpp_inc = cpp_dir.path().join("include");
    let cpp_other = cpp_dir.path().join("other");
    std::fs::create_dir_all(&cpp_src).unwrap();
    std::fs::create_dir_all(&cpp_inc).unwrap();
    std::fs::create_dir_all(&cpp_other).unwrap();

    let use_cpp = cpp_src.join("use.cpp");
    let impl_cpp = cpp_src.join("arch.cpp");
    let good_hpp = cpp_inc.join("arch.hpp");
    let decoy_hpp = cpp_other.join("arch.hpp");

    let good_hpp_text = "#pragma once\nstruct Engine {\n    int compute() const;\n};\n";
    let decoy_hpp_text = "#pragma once\nstruct WrongEngine {};\n";
    let impl_cpp_text = "#include <arch.hpp>\n\nint Engine::compute() const {\n    return 42;\n}\n";
    let use_cpp_text =
        "#include <arch.hpp>\nint run(const Engine& e) {\n    return e.compute();\n}\n";

    std::fs::write(&good_hpp, good_hpp_text).unwrap();
    std::fs::write(&decoy_hpp, decoy_hpp_text).unwrap();
    std::fs::write(&impl_cpp, impl_cpp_text).unwrap();
    std::fs::write(&use_cpp, use_cpp_text).unwrap();

    let compile_db = serde_json::json!([
        {
            "directory": cpp_dir.path().to_string_lossy(),
            "command": format!("clang++ -std=c++17 -Iinclude -c {}", use_cpp.display()),
            "file": use_cpp.to_string_lossy()
        },
        {
            "directory": cpp_dir.path().to_string_lossy(),
            "command": format!("clang++ -std=c++17 -Iinclude -c {}", impl_cpp.display()),
            "file": impl_cpp.to_string_lossy()
        }
    ]);
    std::fs::write(
        cpp_dir.path().join("compile_commands.json"),
        serde_json::to_string_pretty(&compile_db).unwrap(),
    )
    .unwrap();

    let (cpp_tx, cpp_rx) = crossbeam_channel::unbounded();
    let mut cpp_app = App::open_targets(CliTargets {
        workspace: Some(cpp_dir.path().to_path_buf()),
        files: vec![use_cpp.clone()],
        no_config: true,
        ..CliTargets::default()
    })
    .unwrap();
    cpp_app.set_lsp_client(LspClient::new(move |reply| {
        let _ = cpp_tx.send(reply);
    }));

    // Jump on `#include <arch.hpp>` -> opens include/arch.hpp, NOT other/arch.hpp
    let hpp_byte = use_cpp_text.find("arch.hpp").unwrap() + 2;
    cpp_app.editor.cursors_mut().set_single(hpp_byte);
    cpp_app
        .handle_key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE))
        .unwrap();
    wait_and_apply_reply(&mut cpp_app, &cpp_rx);
    let resolved_hpp = cpp_app.editor.file_path().unwrap().to_path_buf();
    println!("[REAL CLANGD C++ #include] -> {}", resolved_hpp.display());
    assert_eq!(
        resolved_hpp.canonicalize().unwrap(),
        good_hpp.canonicalize().unwrap()
    );

    // Jump on `e.compute()` -> wait briefly if background index is still warming up, then verify arch.cpp:3
    cpp_app.editor.switch_tab(0);
    let compute_byte = use_cpp_text.find("compute").unwrap() + 1;
    cpp_app.editor.cursors_mut().set_single(compute_byte);
    let mut jumped_to_impl = false;
    for attempt in 0..10 {
        cpp_app.editor.switch_tab(0);
        cpp_app.editor.cursors_mut().set_single(compute_byte);
        cpp_app
            .handle_key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE))
            .unwrap();
        wait_and_apply_reply(&mut cpp_app, &cpp_rx);
        let dest = cpp_app.editor.file_path().unwrap().canonicalize().unwrap();
        let line = cpp_app.editor.cursor_line_col().0 + 1;
        println!(
            "[REAL CLANGD C++ method attempt {}] -> {}:{}",
            attempt + 1,
            dest.display(),
            line
        );
        if dest == impl_cpp.canonicalize().unwrap() && line == 3 {
            jumped_to_impl = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(
        jumped_to_impl,
        "expected C++ method jump to reach src/arch.cpp:3"
    );
}
