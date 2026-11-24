use crate::editor::Editor;

#[test]
fn test_ctrl_word_motion_repeats_across_words_whitespace_and_punctuation() {
    let mut editor = Editor::from_str("one  two!!! café");

    for expected in [3, 8, 11, 17, 17] {
        editor.move_word_end_forward(false);
        assert_eq!(editor.cursors().primary().head, expected);
    }

    for expected in [12, 8, 5, 0, 0] {
        editor.move_word_backward(false);
        assert_eq!(editor.cursors().primary().head, expected);
    }
}

#[test]
fn test_ctrl_word_motion_preserves_utf8_boundaries_and_selection_anchor() {
    let mut editor = Editor::from_str("olá mundo");

    editor.move_word_end_forward(true);
    assert_eq!(editor.cursors().primary().anchor, 0);
    assert_eq!(editor.cursors().primary().head, 4);

    editor.move_word_end_forward(true);
    assert_eq!(editor.cursors().primary().anchor, 0);
    assert_eq!(editor.cursors().primary().head, 10);

    editor.move_word_backward(true);
    assert_eq!(editor.cursors().primary().anchor, 0);
    assert_eq!(editor.cursors().primary().head, 5);
}

#[test]
fn test_ctrl_word_motion_clamps_empty_start_and_end() {
    let mut empty = Editor::new();
    empty.move_word_end_forward(false);
    empty.move_word_backward(false);
    assert_eq!(empty.cursors().primary().head, 0);

    let mut editor = Editor::from_str("word");
    editor.move_word_backward(false);
    assert_eq!(editor.cursors().primary().head, 0);
    editor.move_word_end_forward(false);
    editor.move_word_end_forward(false);
    assert_eq!(editor.cursors().primary().head, 4);
}

#[test]
fn test_ctrl_right_without_shift_collapses_selection_at_end() {
    let mut editor = Editor::from_str("word");
    editor.move_word_end_forward(true);
    assert_eq!(editor.cursors().primary().range(), 0..4);

    editor.move_word_end_forward(false);
    assert!(!editor.cursors().primary().is_selection());
    assert_eq!(editor.cursors().primary().head, 4);

    editor.insert_char('x').unwrap();
    assert_eq!(editor.buffer().rope().to_string(), "wordx");
}

#[test]
fn wrapped_vertical_and_home_end_motions_step_within_logical_line() {
    let mut e = Editor::from_str("hello world");
    e.move_right(false);
    e.move_down_wrapped(6, true);
    assert_eq!(e.cursor_wrapped_pos(6), (0, 1, 1));
    assert_eq!(e.cursors().primary().anchor, 1);
    e.move_home_wrapped(6, false);
    assert_eq!(e.cursor_wrapped_pos(6), (0, 1, 0));
    e.move_home_wrapped(6, false);
    assert_eq!(e.cursors().primary().head, 0);
    e.move_end_wrapped(6, false);
    assert_eq!(e.cursors().primary().head, 5);
    e.move_down_wrapped(6, false);
    e.move_up_wrapped(6, false);
    assert_eq!(e.cursors().primary().head, 5);
}
