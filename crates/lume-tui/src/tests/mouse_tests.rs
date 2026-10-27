use crate::app::App;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::path::PathBuf;

#[test]
fn test_app_mouse_tab_switching() {
    let mut app = App::new(None).unwrap();
    app.editor.insert_str("tab 1").unwrap();
    app.editor.new_tab();
    app.editor.insert_str("tab 2").unwrap();

    assert_eq!(app.editor.active_tab_idx(), 1);

    // Explorer is visible by default with width = app.text_area_left()
    let offset = app.text_area_left();

    // Click on Tab 1 title area in tab bar (row 0, col 5 inside tab bar)
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: (offset + 5) as u16,
        row: 0,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();

    assert_eq!(app.editor.active_tab_idx(), 0);
}

#[test]
fn test_app_mouse_tab_name_containing_x_does_not_close() {
    let mut app = App::new(None).unwrap();
    app.editor
        .set_file_path(Some(PathBuf::from("/src/explorer.rs")));
    app.editor.new_tab();
    app.editor
        .set_file_path(Some(PathBuf::from("/src/index.html")));

    assert_eq!(app.editor.active_tab_idx(), 1);
    let offset = app.text_area_left();

    // Click on the 'x' character in the file name 'explorer.rs' in Tab 1
    // Tab 1 text: "  1: explorer.rs x   "
    // 'x' in 'explorer' is at offset ~6 in the tab text
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: (offset + 6) as u16,
        row: 0,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();

    // Tab was selected, not closed!
    assert_eq!(app.editor.tabs().len(), 2);
    assert_eq!(app.editor.active_tab_idx(), 0);
}

#[test]
fn test_app_mouse_scroll_preserves_viewport_across_renders() {
    let mut app = App::new(None).unwrap();
    let content = (1..=100)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n");
    app.editor.insert_str(&content).unwrap();

    // Move cursor to line 0
    app.editor.move_lines(-100, false);
    app.adjust_viewport();
    assert_eq!(app.editor.cursor_line_col().0, 0);
    assert_eq!(app.scroll_row, 0);

    // Scroll down via trackpad / mouse
    let text_left = app.text_area_left();
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: text_left as u16 + 5,
        row: 5,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();

    assert_eq!(app.scroll_row, 3);

    // Rendering MUST NOT revert scroll_row back to cursor position (0)
    let mut sink = std::io::sink();
    app.render(&mut sink).unwrap();
    assert_eq!(
        app.scroll_row, 3,
        "render() must not snap scroll_row back to cursor_row"
    );

    // Scroll left/right
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollRight,
        column: text_left as u16 + 5,
        row: 5,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();
    assert_eq!(app.scroll_col, 3);

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollLeft,
        column: text_left as u16 + 5,
        row: 5,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();
    assert_eq!(app.scroll_col, 0);

    // Typing a character brings the viewport back to the cursor
    app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(
        app.scroll_row, 0,
        "Typing must adjust viewport back to cursor"
    );
}

#[test]
fn test_app_mouse_interaction_and_scrolling() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

    let mut app = App::new(None).unwrap();
    app.editor.insert_str("word1 word2\nline2\nline3").unwrap();
    app.terminal_width = 80;
    app.terminal_height = 24;

    let status_row = app.terminal_height.saturating_sub(1);

    // 1. Click on Status bar to trigger language prompt
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 70,
        row: status_row,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();
    assert!(app.prompt.is_some());
    app.prompt = None;

    // 2. Click on Tab Bar
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 30,
        row: 0,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();

    // 3. Click in Editor area (single, double, triple click)
    let text_row = app.text_area_top() as u16;
    let text_col = (app.text_area_left() + 5) as u16;

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: text_col,
        row: text_row,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();
    assert!(app.mouse_dragging);

    // Drag
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: text_col + 3,
        row: text_row,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();

    // Mouse Up
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: text_col + 3,
        row: text_row,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();
    assert!(!app.mouse_dragging);

    // Double click on same spot
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: text_col,
        row: text_row,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();
    assert_eq!(app.click_count, 2);

    // Triple click
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: text_col,
        row: text_row,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();
    assert_eq!(app.click_count, 3);

    // 4. Scroll events
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: text_col,
        row: text_row,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: text_col,
        row: text_row,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollLeft,
        column: text_col,
        row: text_row,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollRight,
        column: text_col,
        row: text_row,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();
}
