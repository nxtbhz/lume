use crate::{
    app::App,
    config::{default_config_toml, AppConfig},
};
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

fn narrow() -> App {
    let mut a = App::new(None).unwrap();
    a.explorer.is_visible = false;
    a.terminal_width = 30;
    a.terminal_height = 10;
    a
}
#[test]
fn default_app_enables_word_wrap_and_renders_without_side_scroll() {
    let mut a = narrow();
    assert!(a.word_wrap);
    let src = "alpha beta gamma delta epsilon zeta";
    a.editor.insert_str(src).unwrap();
    a.adjust_viewport();
    let mut frame = Vec::new();
    a.render(&mut frame).unwrap();
    let text = String::from_utf8(frame).unwrap();
    assert!(src.split_whitespace().all(|w| text.contains(w)));
    assert!(text.contains("  │ "));
    assert_eq!(a.scroll_col, 0);
}
#[test]
fn mouse_click_past_wrapped_subrow_stays_on_clicked_visual_row() {
    let mut a = narrow();
    a.show_line_numbers = false;
    a.terminal_width = 7;
    a.editor.insert_str("hello world").unwrap();
    for (row, col, byte) in [(1, 6, 5), (2, 1, 7)] {
        a.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        })
        .unwrap();
        assert_eq!(a.editor.cursors().primary().head, byte);
    }
}
#[test]
fn giant_single_line_scrolls_across_subrows_and_100k_jump_stays_bounded() {
    let mut a = narrow();
    a.editor = lume_core::Editor::from_str(&"x".repeat(5000));
    for _ in 0..12 {
        a.execute_command("motion.down");
    }
    assert_eq!((a.scroll_row, a.scroll_subrow), (0, 5));
    a.editor = lume_core::Editor::from_str(
        &format!("{}\n", "a paragraph with long lines ".repeat(3)).repeat(99999),
    );
    a.execute_command("motion.document_end");
    let tab = &a.editor.tabs()[0];
    assert_eq!(
        tab.offset_visual_rows((a.scroll_row, a.scroll_subrow), 7, a.active_wrap_cols()),
        (99999, 0)
    );
}
#[test]
fn toggle_word_wrap_command_and_config_roundtrip() {
    let parse = |s: &str| AppConfig::from_toml_str(s, crate::commands::is_valid_action_id).unwrap();
    assert!(parse(&default_config_toml()).editor.word_wrap);
    assert!(!parse("[editor]\nword_wrap=false").editor.word_wrap);
    let mut a = narrow();
    a.execute_command("view.toggle_word_wrap");
    assert!(!a.word_wrap);
}
