use crate::buffer::TextBuffer;
use crate::editor::Editor;

#[test]
fn test_bracket_matching_simple_pairs() {
    let ed = Editor::from_str("(hello) [world] {foo}");
    // '(' at 0 matches ')' at 6
    assert_eq!(ed.find_matching_bracket(0), Some(6));
    // ')' at 6 matches '(' at 0
    assert_eq!(ed.find_matching_bracket(6), Some(0));

    // '[' at 8 matches ']' at 14
    assert_eq!(ed.find_matching_bracket(8), Some(14));
    // ']' at 14 matches '[' at 8
    assert_eq!(ed.find_matching_bracket(14), Some(8));

    // '{' at 16 matches '}' at 20
    assert_eq!(ed.find_matching_bracket(16), Some(20));
    // '}' at 20 matches '{' at 16
    assert_eq!(ed.find_matching_bracket(20), Some(16));
}

#[test]
fn test_bracket_matching_nested_and_siblings() {
    // Indices:
    // { [ ( ) ] }
    // 0 2 4 6 8 10
    let ed = Editor::from_str("{ [ ( ) ] }");
    assert_eq!(ed.find_matching_bracket(0), Some(10));
    assert_eq!(ed.find_matching_bracket(2), Some(8));
    assert_eq!(ed.find_matching_bracket(4), Some(6));
    assert_eq!(ed.find_matching_bracket(6), Some(4));
    assert_eq!(ed.find_matching_bracket(8), Some(2));
    assert_eq!(ed.find_matching_bracket(10), Some(0));

    // Siblings: { () () }
    // 0: {, 2: (, 3: ), 5: (, 6: ), 8: }
    let ed_siblings = Editor::from_str("{ () () }");
    assert_eq!(ed_siblings.find_matching_bracket(0), Some(8));
    assert_eq!(ed_siblings.find_matching_bracket(2), Some(3));
    assert_eq!(ed_siblings.find_matching_bracket(3), Some(2));
    assert_eq!(ed_siblings.find_matching_bracket(5), Some(6));
    assert_eq!(ed_siblings.find_matching_bracket(6), Some(5));
    assert_eq!(ed_siblings.find_matching_bracket(8), Some(0));
}

#[test]
fn test_bracket_matching_unmatched_and_non_bracket() {
    let ed = Editor::from_str("fn foo(bar { baz");
    // '(' at 6 is unclosed
    assert_eq!(ed.find_matching_bracket(6), None);
    // '{' at 11 is unclosed
    assert_eq!(ed.find_matching_bracket(11), None);
    // 'f' at 0 is not a bracket
    assert_eq!(ed.find_matching_bracket(0), None);
    // Out of bounds
    assert_eq!(ed.find_matching_bracket(999), None);
}

#[test]
fn test_bracket_matching_multibyte_utf8() {
    // "Olá (Mundo! 🚀) Fim"
    // 'O'=1, 'l'=1, 'á'=2 -> 4 bytes before ' ' (byte 4) and '(' (byte 5)
    // 🚀 is 4 bytes
    let ed = Editor::from_str("Olá (Mundo! 🚀) Fim");
    let open_pos = ed
        .buffer()
        .slice_to_cow(0, ed.buffer().len_bytes())
        .find('(')
        .unwrap();
    let close_pos = ed
        .buffer()
        .slice_to_cow(0, ed.buffer().len_bytes())
        .find(')')
        .unwrap();

    assert_eq!(ed.find_matching_bracket(open_pos), Some(close_pos));
    assert_eq!(ed.find_matching_bracket(close_pos), Some(open_pos));
}

#[test]
fn test_bracket_matching_jump_motion() {
    let mut ed = Editor::from_str("    let x = (1 + 2);");
    // Start at column 0 (before the bracket on this line)
    ed.cursors_mut().set_single(0);
    // Vim % motion: jumping when not on bracket finds first bracket '(' and jumps to ')'
    let jumped = ed.jump_to_matching_bracket();
    assert!(jumped);
    let close_paren_byte = ed
        .buffer()
        .slice_to_cow(0, ed.buffer().len_bytes())
        .find(')')
        .unwrap();
    assert_eq!(ed.cursors().primary().head, close_paren_byte);

    // Jump again while on ')' jumps back to '('
    let jumped_back = ed.jump_to_matching_bracket();
    assert!(jumped_back);
    let open_paren_byte = ed
        .buffer()
        .slice_to_cow(0, ed.buffer().len_bytes())
        .find('(')
        .unwrap();
    assert_eq!(ed.cursors().primary().head, open_paren_byte);

    // When no brackets on line
    let mut ed_none = Editor::from_str("let x = 42;");
    assert!(!ed_none.jump_to_matching_bracket());
}

#[test]
fn test_bracket_matching_bounded_range() {
    let ed = Editor::from_str("line 0\n{\n    line 2\n}\nline 4");
    // '{' is at line 1, '}' is at line 3
    let open_byte = ed
        .buffer()
        .slice_to_cow(0, ed.buffer().len_bytes())
        .find('{')
        .unwrap();
    let close_byte = ed
        .buffer()
        .slice_to_cow(0, ed.buffer().len_bytes())
        .find('}')
        .unwrap();

    let open_char = ed.buffer().byte_to_char(open_byte);
    let close_char = ed.buffer().byte_to_char(close_byte);

    // If range covers both chars: found
    assert_eq!(
        ed.find_matching_bracket_in_range(open_byte, open_char, close_char + 1),
        Some(close_byte)
    );

    // If range excludes close_char: returns None
    assert_eq!(
        ed.find_matching_bracket_in_range(open_byte, open_char, close_char),
        None
    );

    // If range excludes open_char (start bracket outside range): returns None
    assert_eq!(
        ed.find_matching_bracket_in_range(open_byte, open_char + 1, close_char + 1),
        None
    );

    // Backward scan: if range covers both chars: found
    assert_eq!(
        ed.find_matching_bracket_in_range(close_byte, open_char, close_char + 1),
        Some(open_byte)
    );

    // Backward scan: if range excludes open_char: returns None
    assert_eq!(
        ed.find_matching_bracket_in_range(close_byte, open_char + 1, close_char + 1),
        None
    );

    // Backward scan: if range excludes close_char (close_char >= max_char): returns None
    assert_eq!(
        ed.find_matching_bracket_in_range(close_byte, open_char, close_char),
        None
    );
}
