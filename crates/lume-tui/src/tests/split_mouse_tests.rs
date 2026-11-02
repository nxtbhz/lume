use crate::app::App;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

#[test]
fn test_mouse_click_switches_split_focus() {
    let mut app = App::new(None).unwrap();
    app.explorer.is_visible = false;
    app.terminal_width = 80;
    app.terminal_height = 24;
    app.execute_command("view.split_vertical");
    assert_eq!(app.splits.active_pane, 1);

    // Pane 0 is at cols 0..39. Click at col 15, row 5
    let click_pane0 = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 15,
        row: 5,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    app.handle_mouse(click_pane0).unwrap();
    assert_eq!(
        app.splits.active_pane, 0,
        "Clicking pane 0 must switch active split focus"
    );

    // Click separator at col 39, row 5 -> safe no-op
    let click_sep = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 39,
        row: 5,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    app.handle_mouse(click_sep).unwrap();
    assert_eq!(app.splits.active_pane, 0);

    // Click Pane 1 at col 60, row 5 -> switches to pane 1
    let click_pane1 = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 60,
        row: 5,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    app.handle_mouse(click_pane1).unwrap();
    assert_eq!(app.splits.active_pane, 1);

    // Drag inside active pane
    let drag_pane1 = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 65,
        row: 6,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    app.handle_mouse(drag_pane1).unwrap();
}

#[test]
fn test_mouse_scroll_targets_hovered_pane() {
    let mut app = App::new(None).unwrap();
    app.explorer.is_visible = false;
    app.terminal_width = 80;
    app.terminal_height = 24;
    for _ in 0..50 {
        app.editor.insert_str("line\n").unwrap();
    }
    app.execute_command("view.split_vertical");

    // Scroll down on Pane 0 (hover col 10, row 5)
    let scroll_down_p0 = MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 10,
        row: 5,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    app.handle_mouse(scroll_down_p0).unwrap();
    assert_eq!(app.splits.panes[0].scroll_row, 3);
    assert_eq!(app.splits.panes[1].scroll_row, 0);

    // Scroll up on Pane 0
    let scroll_up_p0 = MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 10,
        row: 5,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    app.handle_mouse(scroll_up_p0).unwrap();
    assert_eq!(app.splits.panes[0].scroll_row, 0);

    // Scroll right and left on Pane 1 (active pane)
    let scroll_right_p1 = MouseEvent {
        kind: MouseEventKind::ScrollRight,
        column: 60,
        row: 5,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    app.handle_mouse(scroll_right_p1).unwrap();
    assert_eq!(app.splits.panes[1].scroll_col, 3);
    assert_eq!(app.scroll_col, 3);

    let scroll_left_p1 = MouseEvent {
        kind: MouseEventKind::ScrollLeft,
        column: 60,
        row: 5,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    app.handle_mouse(scroll_left_p1).unwrap();
    assert_eq!(app.splits.panes[1].scroll_col, 0);
    assert_eq!(app.scroll_col, 0);
}
