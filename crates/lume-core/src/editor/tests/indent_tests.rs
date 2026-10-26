use crate::buffer::TextBuffer;
use crate::cursor::Cursor;
use crate::editor::Editor;

#[test]
fn test_smart_indent_copy_leading_spaces() {
    let mut ed = Editor::new();
    ed.insert_str("    let x = 10;").unwrap();
    ed.insert_newline().unwrap();
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "    let x = 10;\n    "
    );
    assert_eq!(ed.cursor_line_col(), (1, 4));
}

#[test]
fn test_smart_indent_delimiters_add_4_spaces() {
    for delim in &["{", "(", "[", ":"] {
        let mut ed = Editor::new();
        ed.insert_str(&format!("    def test{}", delim)).unwrap();
        ed.insert_newline().unwrap();
        let expected = format!("    def test{}\n        ", delim);
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            expected
        );
        assert_eq!(ed.cursor_line_col(), (1, 8));
    }
}

#[test]
fn test_smart_indent_with_trailing_spaces_after_delimiter() {
    let mut ed = Editor::new();
    ed.insert_str("struct Point {   ").unwrap();
    ed.insert_newline().unwrap();
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "struct Point {   \n    "
    );
    assert_eq!(ed.cursor_line_col(), (1, 4));
}

#[test]
fn test_smart_indent_middle_of_line_split() {
    let mut ed = Editor::new();
    ed.insert_str("    let a = foo(); let b = 2;").unwrap();
    ed.cursors_mut().set_single(18); // Logo após foo();
    ed.insert_newline().unwrap();
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "    let a = foo();\n     let b = 2;"
    );
}

#[test]
fn test_smart_indent_disabled() {
    let mut ed = Editor::new();
    ed.config_mut().smart_indent = false;
    ed.insert_str("    let x = 1;").unwrap();
    ed.insert_newline().unwrap();
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "    let x = 1;\n"
    );
    assert_eq!(ed.cursor_line_col(), (1, 0));
}

#[test]
fn test_smart_indent_undo_atomic() {
    let mut ed = Editor::new();
    ed.insert_str("    fn main() {").unwrap();
    ed.insert_newline().unwrap();
    assert_eq!(ed.cursor_line_col(), (1, 8));
    ed.undo();
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "    fn main() {"
    );
    assert_eq!(ed.cursor_line_col(), (0, 15));
}

#[test]
fn test_smart_unindent_col_4_and_col_8() {
    let mut ed = Editor::new();
    ed.insert_str("        ").unwrap(); // 8 spaces
    assert_eq!(ed.cursor_line_col(), (0, 8));

    // First backspace unindents from col 8 to col 4
    ed.delete_backwards().unwrap();
    assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "    ");
    assert_eq!(ed.cursor_line_col(), (0, 4));

    // Second backspace unindents from col 4 to col 0
    ed.delete_backwards().unwrap();
    assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "");
    assert_eq!(ed.cursor_line_col(), (0, 0));
}

#[test]
fn test_smart_unindent_non_multiple_of_4_deletes_single_space() {
    let mut ed = Editor::new();
    ed.insert_str("      ").unwrap(); // 6 spaces
    assert_eq!(ed.cursor_line_col(), (0, 6));

    // Col 6 is not multiple of 4: deletes 1 space -> 5 spaces
    ed.delete_backwards().unwrap();
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "     "
    );
    assert_eq!(ed.cursor_line_col(), (0, 5));

    // Col 5 -> deletes 1 space -> 4 spaces
    ed.delete_backwards().unwrap();
    assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "    ");
    assert_eq!(ed.cursor_line_col(), (0, 4));

    // Col 4 is multiple of 4 with only spaces -> unindents 4 spaces -> 0 spaces
    ed.delete_backwards().unwrap();
    assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "");
    assert_eq!(ed.cursor_line_col(), (0, 0));
}

#[test]
fn test_smart_unindent_ignores_lines_with_non_space_prefix() {
    let mut ed = Editor::new();
    ed.insert_str("a   ").unwrap(); // 'a' + 3 spaces = col 4
    assert_eq!(ed.cursor_line_col(), (0, 4));

    // Col 4 is multiple of 4, but prefix contains 'a' -> deletes 1 space
    ed.delete_backwards().unwrap();
    assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "a  ");
    assert_eq!(ed.cursor_line_col(), (0, 3));
}

#[test]
fn test_smart_unindent_disabled() {
    let mut ed = Editor::new();
    ed.config_mut().smart_unindent = false;
    ed.insert_str("    ").unwrap();
    ed.delete_backwards().unwrap();
    assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "   ");
    assert_eq!(ed.cursor_line_col(), (0, 3));
}

#[test]
fn test_smart_unindent_with_selection_deletes_selection() {
    let mut ed = Editor::new();
    ed.insert_str("        ").unwrap();
    // Select the last 4 spaces
    ed.cursors_mut().primary_mut().anchor = 4;
    ed.cursors_mut().primary_mut().head = 8;
    ed.delete_backwards().unwrap();
    assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "    ");
    assert_eq!(ed.cursor_line_col(), (0, 4));
}

#[test]
fn test_auto_pairs_triggers_before_whitespace_newline_and_eof() {
    let pairs = [('(', ")"), ('[', "]"), ('{', "}"), ('"', "\""), ('\'', "'")];

    // Case 1: At EOF
    for &(open, close) in &pairs {
        let mut ed = Editor::new();
        ed.insert_char(open).unwrap();
        let expected = format!("{}{}", open, close);
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            expected
        );
        assert_eq!(ed.cursor_line_col(), (0, 1)); // Cursor inside pair
    }

    // Case 2: Before space
    for &(open, close) in &pairs {
        let mut ed = Editor::new();
        ed.insert_str(" ").unwrap();
        ed.cursors_mut().set_single(0);
        ed.insert_char(open).unwrap();
        let expected = format!("{}{} ", open, close);
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            expected
        );
        assert_eq!(ed.cursor_line_col(), (0, 1));
    }

    // Case 3: Before newline
    for &(open, close) in &pairs {
        let mut ed = Editor::new();
        ed.insert_str("\n").unwrap();
        ed.cursors_mut().set_single(0);
        ed.insert_char(open).unwrap();
        let expected = format!("{}{}\n", open, close);
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            expected
        );
        assert_eq!(ed.cursor_line_col(), (0, 1));
    }
}

#[test]
fn test_auto_pairs_does_not_trigger_before_identifier_or_punctuation() {
    let mut ed = Editor::new();
    ed.insert_str("foo;").unwrap();
    ed.cursors_mut().set_single(0);

    // Before 'f': should only insert '('
    ed.insert_char('(').unwrap();
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "(foo;"
    );
    assert_eq!(ed.cursor_line_col(), (0, 1));

    // Move before ';': should only insert '"'
    ed.cursors_mut().set_single(4);
    ed.insert_char('"').unwrap();
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "(foo\";"
    );
}

#[test]
fn test_auto_pairs_overtype_closing_delimiters() {
    let mut ed = Editor::new();
    // Type '(' -> '()' with cursor inside
    ed.insert_char('(').unwrap();
    assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "()");
    assert_eq!(ed.cursor_line_col(), (0, 1));

    // Type ')' -> overtypes, advancing cursor past ')' without inserting duplicate
    ed.insert_char(')').unwrap();
    assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "()");
    assert_eq!(ed.cursor_line_col(), (0, 2));

    // Quotes overtype
    let mut ed2 = Editor::new();
    ed2.insert_char('"').unwrap();
    assert_eq!(ed2.cursor_line_col(), (0, 1));
    ed2.insert_char('"').unwrap();
    assert_eq!(
        ed2.buffer().slice_to_cow(0, ed2.buffer().len_bytes()),
        "\"\""
    );
    assert_eq!(ed2.cursor_line_col(), (0, 2));
}

#[test]
fn test_auto_pairs_disabled() {
    let mut ed = Editor::new();
    ed.config_mut().auto_pairs = false;
    ed.insert_char('(').unwrap();
    assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "(");
    assert_eq!(ed.cursor_line_col(), (0, 1));
}

#[test]
fn test_auto_pairs_undo_atomic() {
    let mut ed = Editor::new();
    ed.insert_char('{').unwrap();
    assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "{}");
    ed.undo();
    assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "");
    assert_eq!(ed.cursor_line_col(), (0, 0));
}

#[test]
fn test_multi_cursor_smart_indent_mixed() {
    let mut ed = Editor::new();
    ed.insert_str("    fn foo() {\n        let x = 1;").unwrap();
    // Cursor 1 at end of line 0 (after '{') -> offset 14
    // Cursor 2 at end of line 1 (after ';') -> offset 33
    ed.cursors_mut()
        .set_cursors(vec![Cursor::new(14), Cursor::new(33)]);

    ed.insert_newline().unwrap();

    let expected = "    fn foo() {\n        \n        let x = 1;\n        ";
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        expected
    );
    assert_eq!(ed.cursors().cursors().len(), 2);
    // Cursor 1 at line 1 col 8
    assert_eq!(ed.cursors().cursors()[0].head, 23);
    // Cursor 2 at line 3 col 8
    assert_eq!(ed.cursors().cursors()[1].head, 51);
}

#[test]
fn test_multi_cursor_smart_unindent() {
    let mut ed = Editor::new();
    ed.insert_str("        \n        ").unwrap();
    // Cursor 1 at end of line 0 (col 8) -> offset 8
    // Cursor 2 at end of line 1 (col 8) -> offset 17
    ed.cursors_mut()
        .set_cursors(vec![Cursor::new(8), Cursor::new(17)]);

    ed.delete_backwards().unwrap();

    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "    \n    "
    );
    assert_eq!(ed.cursors().cursors().len(), 2);
    assert_eq!(ed.cursors().cursors()[0].head, 4);
    assert_eq!(ed.cursors().cursors()[1].head, 9);
}

#[test]
fn test_multi_cursor_auto_pairs() {
    let mut ed = Editor::new();
    ed.insert_str("a \nb ").unwrap();
    // Cursor 1 after 'a ' -> offset 2
    // Cursor 2 after 'b ' -> offset 5
    ed.cursors_mut()
        .set_cursors(vec![Cursor::new(2), Cursor::new(5)]);

    ed.insert_char('(').unwrap();

    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "a ()\nb ()"
    );
    assert_eq!(ed.cursors().cursors().len(), 2);
    assert_eq!(ed.cursors().cursors()[0].head, 3);
    assert_eq!(ed.cursors().cursors()[1].head, 8);
}

#[test]
fn test_smart_unindent_tab_size_zero_does_not_panic() {
    let mut ed = Editor::new();
    ed.config_mut().tab_size = 0;
    ed.insert_str("    ").unwrap();
    // Should not panic with division by zero
    assert!(ed.delete_backwards().is_ok());
}

#[test]
fn test_smart_indent_scope_resolution_operator_does_not_extra_indent() {
    let mut ed = Editor::new();
    ed.insert_str("use std::").unwrap();
    ed.insert_newline().unwrap();
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "use std::\n"
    );
    assert_eq!(ed.cursor_line_col(), (1, 0));
}
