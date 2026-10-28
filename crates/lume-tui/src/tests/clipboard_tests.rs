use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use lume_core::TextBuffer;

use crate::app::App;

#[test]
fn test_app_clipboard_copy_ctrl_c_emits_osc52() {
    let mut app = App::new(None).unwrap();
    app.editor.insert_str("hello clipboard").unwrap();
    app.editor.cursors_mut().primary_mut().anchor = 6;
    app.editor.cursors_mut().primary_mut().head = 15;

    app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
        .unwrap();

    assert_eq!(app.editor.clipboard(), Some("clipboard"));
    assert!(app.pending_osc52.is_some());
    let osc = app.pending_osc52.as_ref().unwrap();
    assert!(osc.contains("]52;c;"));
    assert!(osc.contains(&lume_core::clipboard::encode_base64(b"clipboard")));

    let mut buf = Vec::new();
    app.render(&mut buf).unwrap();
    assert!(app.pending_osc52.is_none());
    let output_str = String::from_utf8_lossy(&buf);
    assert!(output_str.contains("]52;c;"));
}

#[test]
fn test_app_clipboard_cut_ctrl_x_emits_osc52_and_modifies_buffer() {
    let mut app = App::new(None).unwrap();
    app.editor.insert_str("hello world").unwrap();
    app.editor.cursors_mut().primary_mut().anchor = 5;
    app.editor.cursors_mut().primary_mut().head = 11;

    app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL))
        .unwrap();

    assert_eq!(app.editor.clipboard(), Some(" world"));
    assert_eq!(
        app.editor
            .buffer()
            .slice_to_cow(0, app.editor.buffer().len_bytes()),
        "hello"
    );
    assert!(app.pending_osc52.is_some());
}

#[test]
fn test_app_clipboard_paste_ctrl_v() {
    let mut app = App::new(None).unwrap();
    app.editor.set_clipboard("pasted_token");
    app.editor.cursors_mut().set_single(0);

    app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL))
        .unwrap();

    assert_eq!(
        app.editor
            .buffer()
            .slice_to_cow(0, app.editor.buffer().len_bytes()),
        "pasted_token"
    );
}

#[test]
fn test_app_clipboard_command_palette_entries() {
    let mut app = App::new(None).unwrap();
    let commands = app.filtered_commands();
    assert!(commands.iter().any(|c| c.id == "edit.copy"));
    assert!(commands.iter().any(|c| c.id == "edit.cut"));
    assert!(commands.iter().any(|c| c.id == "edit.paste"));

    app.editor.insert_str("abc").unwrap();
    app.editor.cursors_mut().primary_mut().anchor = 0;
    app.editor.cursors_mut().primary_mut().head = 3;

    app.execute_command("edit.copy");
    assert_eq!(app.editor.clipboard(), Some("abc"));
    assert!(app.pending_osc52.is_some());
}

#[test]
fn test_app_vim_yank_syncs_osc52() {
    let mut app = App::new(None).unwrap();
    app.modal_enabled = true;
    app.editor.insert_str("line 1\nline 2\n").unwrap();
    app.editor.cursors_mut().set_single(0);

    // In normal mode: press 'y' then 'y'
    app.handle_modal_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE))
        .unwrap();
    app.handle_modal_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE))
        .unwrap();

    assert_eq!(app.editor.clipboard(), Some("line 1\n"));
    assert!(app.pending_osc52.is_some());
}
