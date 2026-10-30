use crate::app::App;
use crate::tests::TestDir;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::fs;

#[test]
fn test_app_quick_open_lifecycle_and_file_selection() {
    let test_dir = TestDir::new("lume_test_quick_open_lifecycle");
    let root = test_dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    let file1 = root.join("src/first.rs");
    let file2 = root.join("src/second.rs");
    fs::write(&file1, "// first file\n").unwrap();
    fs::write(&file2, "// second file\n").unwrap();

    let mut app = App::from_paths(&[&file1]).unwrap();
    assert_eq!(app.editor.tabs().len(), 1);
    assert!(app.quick_open.is_none());

    // Press Ctrl+P -> Opens Quick Open
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.quick_open.is_some());

    // Render while Quick Open is active
    let mut buf = Vec::new();
    assert!(app.render(&mut buf).is_ok());
    assert!(!buf.is_empty());

    // Type "second"
    for c in "second".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }
    assert_eq!(app.quick_open.as_ref().unwrap().query, "second");

    // Press Enter -> Opens second.rs in new tab
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert!(app.quick_open.is_none());
    assert_eq!(app.editor.tabs().len(), 2);
    assert_eq!(app.editor.file_path().unwrap(), file2.as_path());

    // Press Ctrl+P again, type "first", Enter -> switches to existing tab 0 without duplicating
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL))
        .unwrap();
    for c in "first".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.editor.tabs().len(), 2);
    assert_eq!(app.editor.active_tab_idx(), 0);
    assert_eq!(app.editor.file_path().unwrap(), file1.as_path());
}

#[test]
fn test_app_quick_open_esc_cancels() {
    let mut app = App::new(None).unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.quick_open.is_some());

    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .unwrap();
    assert!(app.quick_open.is_none());
}

#[test]
fn test_app_ctrl_shift_p_opens_command_palette() {
    let mut app = App::new(None).unwrap();
    assert!(!app.palette_open);

    // Ctrl+Shift+P with KeyModifiers::SHIFT opens Command Palette
    app.handle_key(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ))
    .unwrap();
    assert!(app.palette_open);
    assert!(app.quick_open.is_none());

    // Close palette
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .unwrap();
    assert!(!app.palette_open);

    // Ctrl+Shift+P with KeyCode::Char('P') and KeyModifiers::CONTROL also opens Command Palette
    app.handle_key(KeyEvent::new(KeyCode::Char('P'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.palette_open);
    assert!(app.quick_open.is_none());
}

#[test]
fn test_app_quick_open_mouse_click_selects_and_cancels() {
    let test_dir = TestDir::new("lume_test_quick_open_mouse");
    let root = test_dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    let file1 = root.join("src/demo.rs");
    fs::write(&file1, "// demo\n").unwrap();

    let mut app = App::from_paths(&[root.to_path_buf()]).unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.quick_open.is_some());

    // Click outside modal (row 0, col 0) cancels Quick Open
    let click_outside = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    };
    app.handle_mouse(click_outside).unwrap();
    assert!(app.quick_open.is_none());
}

#[test]
fn test_quick_open_render_multibyte_and_overflow_query() {
    let mut app = App::new(None).unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.quick_open.is_some());

    // Type multibyte UTF-8 characters and long string exceeding box width
    for c in "ações_música_super_long_query_string_that_exceeds_modal_box_width_in_terminal".chars()
    {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }

    let mut buf = Vec::new();
    assert!(app.render(&mut buf).is_ok());
    assert!(!buf.is_empty());
}

#[test]
fn test_app_quick_open_mouse_row_click_and_scroll() {
    let test_dir = TestDir::new("lume_test_quick_open_mouse_click");
    let root = test_dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    let file1 = root.join("src/item.rs");
    fs::write(&file1, "// item\n").unwrap();

    let mut app = App::from_paths(&[root.to_path_buf()]).unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.quick_open.is_some());

    // Mouse scroll down and up
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 40,
        row: 5,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();
    assert!(app.quick_open.is_some());

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 40,
        row: 5,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();
    assert!(app.quick_open.is_some());

    // Click on row 3 (first item in modal: start_y = 3, row 3 is 3 + 3 = 6)
    let term_w = app.terminal_width as usize;
    let box_w = (term_w * 3 / 4).clamp(50, 84).min(term_w);
    let start_x = term_w.saturating_sub(box_w) / 2;

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: (start_x + 5) as u16,
        row: 6, // row 6 is inside the list
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();
    assert!(app.quick_open.is_none());
    assert_eq!(app.editor.file_path().unwrap(), file1.as_path());
}
