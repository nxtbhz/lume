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

#[test]
fn test_app_paste_into_search_bar() {
    let mut app = App::new(None).unwrap();
    app.editor.insert_str("target needle here").unwrap();
    app.editor.set_clipboard("needle");

    // Open Find search bar (Ctrl+F)
    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.search_state.is_some());

    // Press Ctrl+V inside search bar
    app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(app.search_state.as_ref().unwrap().query, "needle");
}

#[test]
fn test_app_paste_into_prompt() {
    let mut app = App::new(None).unwrap();
    app.editor.set_clipboard("my_file.rs");

    // Open Open File prompt (Ctrl+O)
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.prompt.is_some());

    // Press Ctrl+V into prompt
    app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(app.prompt.as_ref().unwrap().value, "my_file.rs");
}

#[test]
fn test_app_handle_bracketed_paste_direct() {
    let mut app = App::new(None).unwrap();
    app.handle_paste_text("bracketed text from system");
    assert_eq!(app.editor.clipboard(), Some("bracketed text from system"));
    assert_eq!(
        app.editor
            .buffer()
            .slice_to_cow(0, app.editor.buffer().len_bytes()),
        "bracketed text from system"
    );
}

#[test]
fn test_app_handle_bracketed_paste_into_command_palette() {
    let mut app = App::new(None).unwrap();
    app.editor.insert_str("initial content").unwrap();
    app.palette_open = true;
    app.palette_query.clear();
    app.palette_selected = 5;

    app.handle_paste_text("save");

    assert_eq!(app.palette_query, "save");
    assert_eq!(app.palette_selected, 0);
    assert_eq!(
        app.editor
            .buffer()
            .slice_to_cow(0, app.editor.buffer().len_bytes()),
        "initial content"
    );
}

#[test]
fn test_app_event_paste_dispatches_and_renders() {
    use crate::input::AppEvent;

    let mut app = App::new(None).unwrap();
    let mut out = Vec::new();
    app.handle_event(AppEvent::Paste("event paste payload".to_string()), &mut out)
        .unwrap();

    assert_eq!(app.editor.clipboard(), Some("event paste payload"));
    assert_eq!(
        app.editor
            .buffer()
            .slice_to_cow(0, app.editor.buffer().len_bytes()),
        "event paste payload"
    );
    assert!(!out.is_empty());
}

#[test]
fn test_app_ctrl_v_in_command_palette() {
    let mut app = App::new(None).unwrap();
    app.editor.set_clipboard("quit");
    app.palette_open = true;
    app.palette_query = "app.".to_string();
    app.palette_selected = 3;

    // Press Ctrl+V inside palette
    app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL))
        .unwrap();

    assert_eq!(app.palette_query, "app.quit");
    assert_eq!(app.palette_selected, 0);
}

#[test]
fn test_app_ctrl_v_in_replace_replacement_field() {
    use crate::search::{SearchField, SearchMode};

    let mut app = App::new(None).unwrap();
    app.editor.set_clipboard("new_val");

    // Open replace dialog (Ctrl+H)
    app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.search_state.is_some());

    let state = app.search_state.as_mut().unwrap();
    state.mode = SearchMode::Replace;
    state.active_field = SearchField::Replacement;

    // Press Ctrl+V in replacement field
    app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL))
        .unwrap();

    assert_eq!(app.search_state.as_ref().unwrap().replacement, "new_val");
}

#[test]
fn test_app_vim_visual_yank_and_cut_sync_osc52() {
    use crate::vim::VimMode;

    let mut app = App::new(None).unwrap();
    app.modal_enabled = true;
    app.editor.insert_str("apple banana cherry").unwrap();
    app.editor.cursors_mut().primary_mut().anchor = 6;
    app.editor.cursors_mut().primary_mut().head = 12; // "banana"
    app.vim_state.set_mode(VimMode::Visual);

    // Press 'y' in visual mode
    app.handle_modal_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE))
        .unwrap();

    assert_eq!(app.editor.clipboard(), Some("banana"));
    assert!(app.pending_osc52.is_some());
    assert_eq!(app.vim_state.mode, VimMode::Normal);
    app.pending_osc52 = None;

    // Now test visual cut ('d')
    app.editor.cursors_mut().primary_mut().anchor = 0;
    app.editor.cursors_mut().primary_mut().head = 5; // "apple"
    app.vim_state.set_mode(VimMode::Visual);
    app.handle_modal_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE))
        .unwrap();

    assert_eq!(app.editor.clipboard(), Some("apple"));
    assert!(app.pending_osc52.is_some());
    assert_eq!(app.vim_state.mode, VimMode::Normal);
    app.pending_osc52 = None;

    // Now test normal mode 'dd' (cut line)
    app.handle_modal_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE))
        .unwrap();
    app.handle_modal_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE))
        .unwrap();

    assert_eq!(app.editor.clipboard(), Some(" banana cherry\n"));
    assert!(app.pending_osc52.is_some());
}

#[test]
fn test_app_paste_multiline_sanitized_in_overlays() {
    use crate::search::{SearchField, SearchMode};

    let mut app = App::new(None).unwrap();

    // 1. Bracketed paste into command palette
    app.palette_open = true;
    app.palette_query.clear();
    app.handle_paste_text("first_line\nsecond_line");
    assert_eq!(app.palette_query, "first_line");
    assert!(!app.palette_query.contains('\n'));
    app.palette_open = false;

    // 2. Paste into prompt
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.prompt.is_some());
    app.editor.set_clipboard("my_file.rs\nignored_line");
    app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(app.prompt.as_ref().unwrap().value, "my_file.rs");
    assert!(!app.prompt.as_ref().unwrap().value.contains('\n'));
    app.prompt = None;

    // 3. Paste into search state query and replacement
    app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.search_state.is_some());
    app.editor.set_clipboard("search_target\nignored");
    app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(app.search_state.as_ref().unwrap().query, "search_target");
    assert!(!app.search_state.as_ref().unwrap().query.contains('\n'));

    // Switch to replacement field
    let state = app.search_state.as_mut().unwrap();
    state.mode = SearchMode::Replace;
    state.active_field = SearchField::Replacement;
    app.editor.set_clipboard("replace_target\nignored");
    app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(
        app.search_state.as_ref().unwrap().replacement,
        "replace_target"
    );
    assert!(!app
        .search_state
        .as_ref()
        .unwrap()
        .replacement
        .contains('\n'));
}
