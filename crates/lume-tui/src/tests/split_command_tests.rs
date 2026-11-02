use crate::app::App;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn test_split_commands_execution() {
    let mut app = App::new(None).unwrap();
    assert!(!app.splits.is_split);

    // Split vertical
    app.execute_command("view.split_vertical");
    assert!(app.splits.is_split);
    assert_eq!(app.splits.direction, crate::split::SplitDirection::Vertical);
    assert_eq!(app.splits.active_pane, 1);

    // Toggle focus
    app.execute_command("view.focus_next_split");
    assert_eq!(app.splits.active_pane, 0);

    // Split horizontal switches direction
    app.execute_command("view.split_horizontal");
    assert_eq!(
        app.splits.direction,
        crate::split::SplitDirection::Horizontal
    );

    // Close split
    app.execute_command("view.close_split");
    assert!(!app.splits.is_split);
}

#[test]
fn test_ctrl_k_chord_lifecycle_and_cancellation() {
    let mut app = App::new(None).unwrap();

    // 1. Press Ctrl+K
    let ctrl_k = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL);
    app.handle_key(ctrl_k).unwrap();
    assert_eq!(app.pending_chord, Some('k'));

    // 2. Press '\' -> split vertical
    let backslash = KeyEvent::new(KeyCode::Char('\\'), KeyModifiers::NONE);
    app.handle_key(backslash).unwrap();
    assert_eq!(app.pending_chord, None);
    assert!(app.splits.is_split);

    // 3. Press Ctrl+K then Esc -> cancels chord
    app.handle_key(ctrl_k).unwrap();
    assert_eq!(app.pending_chord, Some('k'));
    let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    app.handle_key(esc).unwrap();
    assert_eq!(app.pending_chord, None);

    // 4. Press Ctrl+K then unknown key 'x' -> resets chord without stuck state
    app.handle_key(ctrl_k).unwrap();
    let x = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
    app.handle_key(x).unwrap();
    assert_eq!(app.pending_chord, None);

    // 5. Press Ctrl+K then '-' -> horizontal split
    app.handle_key(ctrl_k).unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('-'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(
        app.splits.direction,
        crate::split::SplitDirection::Horizontal
    );

    // 6. Press Ctrl+K then 'w' -> focus next split
    let prev_pane = app.splits.active_pane;
    app.handle_key(ctrl_k).unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE))
        .unwrap();
    assert_ne!(app.splits.active_pane, prev_pane);

    // 7. Press Ctrl+K then 'q' -> close split
    app.handle_key(ctrl_k).unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
        .unwrap();
    assert!(!app.splits.is_split);

    // 8. Press Ctrl+K then 'm' -> language mode prompt
    app.handle_key(ctrl_k).unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE))
        .unwrap();
    assert!(app.prompt.is_some());
}
