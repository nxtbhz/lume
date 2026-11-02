use crate::app::App;

#[test]
fn test_render_split_vertical_and_separator() {
    let mut app = App::new(None).unwrap();
    app.terminal_width = 80;
    app.terminal_height = 24;
    app.show_line_numbers = false;
    app.editor.insert_str("left content\nsecond line").unwrap();

    // Trigger split vertical
    app.execute_command("view.split_vertical");

    let mut output = Vec::new();
    app.render(&mut output).unwrap();
    let rendered = String::from_utf8_lossy(&output);

    // Must contain vertical separator character and render both panes
    assert!(
        rendered.contains('│'),
        "Rendered split output must contain vertical separator"
    );
    assert_eq!(rendered.matches("left content").count(), 2);
}

#[test]
fn test_render_split_horizontal_and_separator() {
    let mut app = App::new(None).unwrap();
    app.terminal_width = 80;
    app.terminal_height = 24;
    app.editor.insert_str("top content\nsecond line").unwrap();

    // Trigger split horizontal
    app.execute_command("view.split_horizontal");

    let mut output = Vec::new();
    app.render(&mut output).unwrap();
    let rendered = String::from_utf8_lossy(&output);

    // Must contain horizontal separator character
    assert!(
        rendered.contains('─'),
        "Rendered split output must contain horizontal separator"
    );
}

#[test]
fn test_inactive_pane_does_not_mirror_active_bracket_highlight() {
    let mut app = App::new(None).unwrap();
    app.explorer.is_visible = false;
    app.terminal_width = 80;
    app.terminal_height = 24;
    app.show_line_numbers = false;
    app.editor.insert_str("(hello)").unwrap();
    // Place cursor on '(' at byte 0 so bracket matching highlights '(' and ')'
    app.editor.set_cursor_from_line_col(0, 0, false);

    // Split vertically (both panes view Tab 0; active_pane == 1)
    app.execute_command("view.split_vertical");
    // Move Pane 0's cursor away from the bracket (byte 2, 'e')
    app.splits.panes[0].cursors.set_single(2);

    let mut output = Vec::new();
    app.render(&mut output).unwrap();
    let rendered = String::from_utf8_lossy(&output);

    // Bracket highlight background RGB (70, 70, 110) -> "\x1b[48;2;70;70;110m"
    // Active pane (pane 1) has 2 highlighted chars ('(' and ')'); inactive pane (pane 0) must have 0
    assert_eq!(
        rendered.matches("48;2;70;70;110").count(),
        2,
        "Only the active pane should render matching bracket highlights for its cursor"
    );
}
