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
