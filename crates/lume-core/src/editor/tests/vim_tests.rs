use crate::buffer::TextBuffer;
use crate::editor::Editor;

#[test]
fn test_vim_primitives() {
    let mut editor = Editor::new();
    editor
        .insert_str("hello world 123\nline two\nline three\n")
        .unwrap();

    // 1. Navigation: move_to_start & move_to_end
    editor.move_to_end(false);
    assert_eq!(editor.cursors().primary().head, editor.buffer().len_bytes());
    editor.move_to_start(false);
    assert_eq!(editor.cursors().primary().head, 0);

    // 2. Navigation: move_word_forward & move_word_backward
    editor.move_word_forward(false);
    assert_eq!(editor.cursors().primary().head, 6); // start of "world"
    editor.move_word_forward(false);
    assert_eq!(editor.cursors().primary().head, 12); // start of "123"
    editor.move_word_forward(false);
    assert_eq!(editor.cursors().primary().head, 16); // start of "line two"

    editor.move_word_backward(false);
    assert_eq!(editor.cursors().primary().head, 12); // back to "123"
    editor.move_word_backward(false);
    assert_eq!(editor.cursors().primary().head, 6); // back to "world"
    editor.move_word_backward(false);
    assert_eq!(editor.cursors().primary().head, 0); // back to "hello"
    editor.move_word_backward(false);
    assert_eq!(editor.cursors().primary().head, 0); // clamp at 0

    // 3. Selection with word movement
    editor.move_word_forward(true);
    assert!(editor.cursors().primary().is_selection());
    assert_eq!(editor.cursors().primary().anchor, 0);
    assert_eq!(editor.cursors().primary().head, 6);

    // 4. Yank line
    editor.move_to_start(false);
    let yanked = editor.yank_line();
    assert_eq!(yanked, "hello world 123\n");

    // 5. Delete line & atomic undo
    editor.move_word_forward(false); // inside line 1
    let deleted = editor.delete_line().unwrap();
    assert_eq!(deleted, "hello world 123\n");
    assert_eq!(editor.buffer().rope().to_string(), "line two\nline three\n");
    assert_eq!(editor.cursors().primary().head, 0);

    assert!(editor.undo());
    assert_eq!(
        editor.buffer().rope().to_string(),
        "hello world 123\nline two\nline three\n"
    );

    // 6. Paste yank: linewise
    editor.move_to_start(false);
    editor.paste_yank("new second line\n", true).unwrap();
    assert_eq!(
        editor.buffer().rope().to_string(),
        "hello world 123\nnew second line\nline two\nline three\n"
    );
    assert!(editor.undo());
    assert_eq!(
        editor.buffer().rope().to_string(),
        "hello world 123\nline two\nline three\n"
    );

    // 7. Paste yank: characterwise
    editor.move_to_start(false);
    editor.move_right(false); // at 'e'
    editor.paste_yank("XYZ", false).unwrap();
    assert_eq!(
        editor.buffer().rope().to_string(),
        "heXYZllo world 123\nline two\nline three\n"
    );
    assert!(editor.undo());
    assert_eq!(
        editor.buffer().rope().to_string(),
        "hello world 123\nline two\nline three\n"
    );
}

#[test]
fn test_vim_primitives_utf8_and_empty_lines() {
    let mut editor = Editor::new();
    // Empty buffer tests
    assert_eq!(editor.delete_line().unwrap(), "");
    assert_eq!(editor.yank_line(), "");
    editor.paste_yank("first line\n", true).unwrap();
    assert_eq!(editor.buffer().rope().to_string(), "first line\n");

    // UTF-8 Multibyte text
    editor.delete_line().unwrap();
    editor.insert_str("olá mundo_123 çã!\n\né isso\n").unwrap();
    editor.move_to_start(false);

    // Forward word movements across accents and punctuation
    editor.move_word_forward(false); // to "mundo_123"
    editor.move_word_forward(false); // to "çã"
    editor.move_word_forward(false); // to "!"
    editor.move_word_forward(false); // to "\n\n" empty line / "é"
    editor.move_word_forward(false); // to "isso"

    // Backward word movements
    editor.move_word_backward(false); // to "é"
    editor.move_word_backward(false); // to "!"
    editor.move_word_backward(false); // to "çã"
    editor.move_word_backward(false); // to "mundo_123"
    editor.move_word_backward(false); // to "olá"
    assert_eq!(editor.cursors().primary().head, 0);

    // Delete line without trailing newline
    editor.move_to_end(false);
    editor.insert_str("last line without newline").unwrap();
    let deleted_last = editor.delete_line().unwrap();
    assert!(deleted_last.contains("last line without newline"));
}

#[test]
fn test_delete_line_eof_and_paste_normalization() {
    let mut editor = Editor::new();
    editor.insert_str("line 1\nline 2\n").unwrap();
    editor.move_to_end(false);
    let deleted = editor.delete_line().unwrap();
    assert_eq!(deleted, "line 2\n");
    assert_eq!(editor.buffer().rope().to_string(), "line 1\n");

    let mut editor2 = Editor::new();
    editor2.insert_str("line A\nline B").unwrap();
    editor2.move_to_end(false);
    let deleted2 = editor2.delete_line().unwrap();
    assert_eq!(deleted2, "line B\n");
    assert_eq!(editor2.buffer().rope().to_string(), "line A");

    editor2.move_to_start(false);
    editor2.paste_yank(&deleted2, true).unwrap();
    assert_eq!(editor2.buffer().rope().to_string(), "line A\nline B\n");
}
