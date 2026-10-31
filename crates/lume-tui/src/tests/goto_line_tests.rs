use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crate::app::App;
use crate::tests::TestDir;

fn enter_key() -> KeyEvent {
    KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
}

fn esc_key() -> KeyEvent {
    KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)
}

fn type_str(app: &mut App, s: &str) {
    for c in s.chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }
}

#[test]
fn test_goto_line_and_col_basic_jump() {
    let test_dir = TestDir::new("lume_goto_basic");
    let mut app = App::new(Some(test_dir.path().to_path_buf())).unwrap();
    let file = test_dir.path().join("code.rs");
    std::fs::write(&file, "line 1\nline 2 second\nline 3 third\nline 4 fourth\n").unwrap();
    app.editor.open_or_load(&file).unwrap();

    // 1. Jump to line 3 (format "3")
    app.execute_command("navigation.goto_line");
    type_str(&mut app, "3");
    app.handle_key(enter_key()).unwrap();

    let (line, col) = app.editor.cursor_line_col();
    assert_eq!((line, col), (2, 0)); // 0-indexed: line 3, col 1
    assert!(app.status_message.as_ref().unwrap().0.contains("Jumped to line 3, col 1"));

    // 2. Jump to line 2, col 6 (format "2:6")
    app.execute_command("navigation.goto_line");
    type_str(&mut app, "2:6");
    app.handle_key(enter_key()).unwrap();

    let (line, col) = app.editor.cursor_line_col();
    assert_eq!((line, col), (1, 5)); // 0-indexed: line 2, col 6
    assert!(app.status_message.as_ref().unwrap().0.contains("Jumped to line 2, col 6"));

    // 3. Jump relative column ":3" keeping current line
    app.execute_command("navigation.goto_line");
    type_str(&mut app, ":3");
    app.handle_key(enter_key()).unwrap();

    let (line, col) = app.editor.cursor_line_col();
    assert_eq!((line, col), (1, 2)); // 0-indexed: line 2, col 3
    assert!(app.status_message.as_ref().unwrap().0.contains("Jumped to line 2, col 3"));
}

#[test]
fn test_goto_line_clamping_and_utf8() {
    let test_dir = TestDir::new("lume_goto_clamp");
    let mut app = App::new(Some(test_dir.path().to_path_buf())).unwrap();
    let file = test_dir.path().join("unicode.rs");
    // 3 lines without trailing newline: line 1, line 2, line 3
    std::fs::write(&file, "ação_música\nsecond line\nthird line").unwrap();
    app.editor.open_or_load(&file).unwrap();

    // Line clamping: line 999999 clamps to last line (line 3, 0-indexed 2)
    app.execute_command("navigation.goto_line");
    type_str(&mut app, "999999");
    app.handle_key(enter_key()).unwrap();

    let (line, _) = app.editor.cursor_line_col();
    assert_eq!(line, 2); // line 3 (0-indexed 2)
    assert!(app.status_message.as_ref().unwrap().0.contains("Jumped to line 3, col 1"));

    // Column clamping: col 500 on line 2 clamps to end of line 2
    app.execute_command("navigation.goto_line");
    type_str(&mut app, "2:500");
    app.handle_key(enter_key()).unwrap();

    let (line, col) = app.editor.cursor_line_col();
    assert_eq!(line, 1);
    assert_eq!(col, "second line".len());

    // Multibyte UTF-8: jump to col 6 on line 1 ("ação_música")
    app.execute_command("navigation.goto_line");
    type_str(&mut app, "1:6");
    app.handle_key(enter_key()).unwrap();

    let (line, col) = app.editor.cursor_line_col();
    assert_eq!((line, col), (0, 5));
}

#[test]
fn test_goto_line_multi_cursor_collapse_and_invalid_syntax() {
    let test_dir = TestDir::new("lume_goto_multi");
    let mut app = App::new(Some(test_dir.path().to_path_buf())).unwrap();
    let file = test_dir.path().join("multi.txt");
    std::fs::write(&file, "one\ntwo\nthree\n").unwrap();
    app.editor.open_or_load(&file).unwrap();

    // Create multiple cursors
    app.editor.cursors_mut().set_cursors(vec![
        lume_core::cursor::Cursor::new(0),
        lume_core::cursor::Cursor::new(4),
    ]);
    assert_eq!(app.editor.cursors().cursors().len(), 2);

    // Jump to line 2 collapses multi-cursors
    app.execute_command("navigation.goto_line");
    type_str(&mut app, "2");
    app.handle_key(enter_key()).unwrap();

    assert_eq!(app.editor.cursors().cursors().len(), 1);
    let (line, col) = app.editor.cursor_line_col();
    assert_eq!((line, col), (1, 0));

    // Invalid syntax leaves cursor intact and warns in status
    app.execute_command("navigation.goto_line");
    type_str(&mut app, "invalid_num");
    app.handle_key(enter_key()).unwrap();

    let (line, col) = app.editor.cursor_line_col();
    assert_eq!((line, col), (1, 0));
    assert!(app.status_message.as_ref().unwrap().0.contains("Invalid line[:col] format"));

    // Cancel with Esc leaves cursor intact and sets status Cancelled
    app.execute_command("navigation.goto_line");
    type_str(&mut app, "3");
    app.handle_key(esc_key()).unwrap();
    assert!(app.prompt.is_none());
    let (line, col) = app.editor.cursor_line_col();
    assert_eq!((line, col), (1, 0));
    assert!(app.status_message.as_ref().unwrap().0.contains("Cancelled"));
}
