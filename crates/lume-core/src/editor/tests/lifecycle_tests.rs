use crate::buffer::TextBuffer;
use crate::cursor::Cursor;
use crate::editor::Editor;

#[test]
fn test_editor_replace_current_and_undo() {
    let mut ed = Editor::from_str("foo bar foo baz");
    // Find and select first match 0..3
    ed.find_next("foo", true);
    assert_eq!(ed.cursors().primary().range(), 0..3);
    let replaced = ed.replace_current("foo", "qux", true);
    assert!(replaced);
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "qux bar foo baz"
    );
    // Cursor should have jumped to next match
    assert_eq!(ed.cursors().primary().range(), 8..11);

    ed.undo();
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "foo bar foo baz"
    );
}

#[test]
fn test_editor_replace_current_preview_when_unselected() {
    let mut ed = Editor::from_str("foo bar foo baz");
    // Place cursor at offset 4 (on "bar", not selecting "foo")
    ed.cursors_mut().set_single(4);
    let replaced = ed.replace_current("foo", "qux", true);
    // Should NOT destructively replace yet; previews by selecting next match at 8..11
    assert!(!replaced);
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "foo bar foo baz"
    );
    assert_eq!(ed.cursors().primary().range(), 8..11);

    // Subsequent replace_current with match selected replaces it
    let replaced_now = ed.replace_current("foo", "qux", true);
    assert!(replaced_now);
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "foo bar qux baz"
    );
}

#[test]
fn test_editor_replace_all_50_plus_atomic_and_multibyte_asymmetric() {
    let mut initial = String::new();
    for i in 0..60 {
        initial.push_str(&format!("item_ação_{} ", i));
    }
    let mut ed = Editor::from_str(&initial);
    let count = ed.replace_all("item_ação", "🚀_novo_item_longo", true);
    assert_eq!(count, 60);

    // Single atomic undo restores exact original string
    assert!(ed.undo());
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        initial
    );

    // Redo restores all 60
    assert!(ed.redo());
    assert!(!ed
        .buffer()
        .slice_to_cow(0, ed.buffer().len_bytes())
        .contains("item_ação"));
}

#[test]
fn test_editor_replace_all_atomic_undo() {
    let mut ed = Editor::from_str("let x = alpha + alpha;\nlet y = alpha;");
    let count = ed.replace_all("alpha", "beta_val", true);
    assert_eq!(count, 3);
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "let x = beta_val + beta_val;\nlet y = beta_val;"
    );

    // Atomic undo: single undo call must revert all 3 replacements
    assert!(ed.undo());
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "let x = alpha + alpha;\nlet y = alpha;"
    );

    // Redo restores all 3 replacements
    assert!(ed.redo());
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "let x = beta_val + beta_val;\nlet y = beta_val;"
    );
}

#[test]
fn test_editor_typing_and_navigation() {
    let mut ed = Editor::new();
    ed.insert_str("line 1\nline 2").unwrap();

    assert_eq!(ed.cursor_line_col(), (1, 6));

    ed.move_left(false);
    assert_eq!(ed.cursor_line_col(), (1, 5));

    ed.move_up(false);
    assert_eq!(ed.cursor_line_col(), (0, 5));

    ed.move_home(false);
    assert_eq!(ed.cursor_line_col(), (0, 0));

    ed.move_end(false);
    assert_eq!(ed.cursor_line_col(), (0, 6));

    ed.delete_backwards().unwrap();
    assert_eq!(ed.cursor_line_col(), (0, 5));
    assert!(ed
        .buffer()
        .slice_to_cow(0, ed.buffer().len_bytes())
        .starts_with("line \n"));
}

#[test]
fn test_editor_undo_redo() {
    let mut ed = Editor::new();
    ed.insert_str("abc").unwrap();
    assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "abc");

    ed.undo();
    assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "");

    ed.redo();
    assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "abc");
}

#[test]
fn test_editor_multi_tabs() {
    let mut ed = Editor::new();
    ed.insert_str("tab 1 content").unwrap();

    let tab2_idx = ed.new_tab();
    assert_eq!(tab2_idx, 1);
    ed.insert_str("tab 2 content").unwrap();

    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "tab 2 content"
    );

    ed.switch_tab(0);
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "tab 1 content"
    );

    ed.close_tab(0);
    assert_eq!(ed.tabs().len(), 1);
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "tab 2 content"
    );
}

#[test]
fn test_editor_mouse_word_and_line_selection() {
    let mut ed = Editor::new();
    ed.insert_str("hello_world and goodbye\nsecond line")
        .unwrap();

    // Click inside "hello_world"
    ed.set_cursor_from_line_col(0, 4, false);
    ed.select_word_at_cursor();
    let primary = ed.cursors().primary();
    assert!(primary.is_selection());
    assert_eq!(primary.min(), 0);
    assert_eq!(primary.max(), 11);

    // Triple click on second line
    ed.set_cursor_from_line_col(1, 3, false);
    ed.select_line_at_cursor();
    let primary = ed.cursors().primary();
    assert!(primary.is_selection());
    assert_eq!(primary.min(), 24);
    assert_eq!(primary.max(), 35);
}

#[test]
fn test_editor_overwrite_selection_single_undo() {
    let mut ed = Editor::new();
    ed.insert_str("Hello World").unwrap();

    // Select "World" (bytes 6..11)
    ed.cursors_mut().primary_mut().anchor = 6;
    ed.cursors_mut().primary_mut().head = 11;
    assert!(ed.cursors().primary().is_selection());

    // Overwrite selection with "Rust"
    ed.insert_str("Rust").unwrap();
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "Hello Rust"
    );

    // A SINGLE undo must restore "Hello World" and original selection!
    ed.undo();
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "Hello World"
    );
    let primary = ed.cursors().primary();
    assert_eq!(primary.min(), 6);
    assert_eq!(primary.max(), 11);

    // Redo restores "Hello Rust"
    ed.redo();
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "Hello Rust"
    );
}

#[test]
fn test_editor_multi_cursor_simultaneous_editing_and_undo() {
    let mut ed = Editor::new();
    ed.insert_str("alpha beta gamma").unwrap();

    // Set 3 cursors at the end of each word: after 'alpha' (5), after 'beta' (10), after 'gamma' (16)
    ed.cursors_mut()
        .set_cursors(vec![Cursor::new(5), Cursor::new(10), Cursor::new(16)]);
    assert_eq!(ed.cursors().cursors().len(), 3);

    // Simultaneous insert "_test" at all 3 cursors
    ed.insert_str("_test").unwrap();
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "alpha_test beta_test gamma_test"
    );

    // All 3 cursors should still exist and be properly positioned
    assert_eq!(ed.cursors().cursors().len(), 3);
    assert_eq!(ed.cursors().cursors()[0].head, 10); // 5 + 5
    assert_eq!(ed.cursors().cursors()[1].head, 20); // 10 + 5 + 5
    assert_eq!(ed.cursors().cursors()[2].head, 31); // 16 + 5 + 5 + 5

    // Simultaneous delete backwards (delete 5 chars of "_test")
    for _ in 0..5 {
        ed.delete_backwards().unwrap();
    }
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "alpha beta gamma"
    );
    assert_eq!(ed.cursors().cursors().len(), 3);
    assert_eq!(ed.cursors().cursors()[0].head, 5);
    assert_eq!(ed.cursors().cursors()[1].head, 10);
    assert_eq!(ed.cursors().cursors()[2].head, 16);

    // Undo 5 deletions, then undo insert
    for _ in 0..5 {
        ed.undo();
    }
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "alpha_test beta_test gamma_test"
    );

    // One more undo reverses the multi-cursor insertion
    ed.undo();
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "alpha beta gamma"
    );
    assert_eq!(ed.cursors().cursors().len(), 3);
    assert_eq!(ed.cursors().cursors()[0].head, 5);
    assert_eq!(ed.cursors().cursors()[1].head, 10);
    assert_eq!(ed.cursors().cursors()[2].head, 16);
}

#[test]
fn test_multi_cursor_out_of_bounds_selection_delta_offset() {
    let mut ed = Editor::new();
    ed.insert_str("0123456789").unwrap();

    ed.cursors_mut()
        .set_cursors(vec![Cursor::new(2), Cursor::new(5)]);
    // Mutate primary cursor selection past the end of the buffer
    ed.cursors_mut().primary_mut().head = 20;

    // Inserting "X" skips out-of-bounds deletion on cursor 1 and must only shift cursor 2 by +1
    ed.insert_str("X").unwrap();
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "01X234X56789"
    );
}

#[test]
fn test_editor_page_up_and_page_down() {
    let mut ed = Editor::new();
    let content = (1..=50)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n");
    ed.insert_str(&content).unwrap();

    // Cursor at bottom
    assert_eq!(ed.cursor_line_col().0, 49);

    // Page up by 10 lines
    ed.page_up(10, false);
    assert_eq!(ed.cursor_line_col().0, 39);

    // Page up by 100 lines (clamps to line 0)
    ed.page_up(100, false);
    assert_eq!(ed.cursor_line_col().0, 0);

    // Page down by 15 lines
    ed.page_down(15, false);
    assert_eq!(ed.cursor_line_col().0, 15);

    // Page down past end (clamps to last line 49)
    ed.page_down(100, false);
    assert_eq!(ed.cursor_line_col().0, 49);

    // Selection with shift
    ed.page_up(10, true);
    assert_eq!(ed.cursor_line_col().0, 39);
    assert!(ed.cursors().primary().is_selection());
}

#[test]
fn test_editor_delete_forwards_and_multibyte() {
    let mut editor = Editor::new();
    editor.insert_str("abcdef").unwrap();
    editor.cursors_mut().set_single(2); // between b and c

    // Single delete forwards: deletes 'c'
    editor.delete_forwards().unwrap();
    assert_eq!(
        editor.buffer().slice_to_cow(0, editor.buffer().len_bytes()),
        "abdef"
    );

    // Selection delete forwards
    editor
        .cursors_mut()
        .set_cursors(vec![Cursor::with_anchor(1, 3)]); // "bd"
    editor.delete_forwards().unwrap();
    assert_eq!(
        editor.buffer().slice_to_cow(0, editor.buffer().len_bytes()),
        "aef"
    );

    // Delete forwards at EOF
    editor.cursors_mut().set_single(3);
    assert!(editor.delete_forwards().is_ok());
    assert_eq!(editor.buffer().len_bytes(), 3);

    // Delete forwards with multibyte emoji
    let mut ed_utf8 = Editor::new();
    ed_utf8.insert_str("A🚀B").unwrap();
    ed_utf8.cursors_mut().set_single(1); // right before 🚀
    ed_utf8.delete_forwards().unwrap();
    assert_eq!(
        ed_utf8
            .buffer()
            .slice_to_cow(0, ed_utf8.buffer().len_bytes()),
        "AB"
    );
}

#[test]
fn test_editor_selection_movements_and_tab_cycle() {
    let mut editor = Editor::new();
    editor
        .insert_str("line 1\nline 2 with more words\nline 3")
        .unwrap();
    editor.cursors_mut().set_single(0);

    // move_right with selection
    editor.move_right(true);
    editor.move_right(true);
    assert_eq!(editor.cursors().primary().anchor, 0);
    assert_eq!(editor.cursors().primary().head, 2);

    // move_left collapses selection when select: false
    editor.move_left(false);
    assert!(!editor.cursors().primary().is_selection());

    // move_down with selection
    editor.move_down(true);
    assert!(editor.cursors().primary().is_selection());

    // move_up with selection
    editor.move_up(true);

    // move_end and move_home with selection
    editor.move_end(true);
    editor.move_home(true);

    // Boundary movements
    editor.cursors_mut().set_single(0);
    editor.move_up(false); // at line 0
    assert_eq!(editor.cursors().primary().head, 0);

    let end_pos = editor.buffer().len_bytes();
    editor.cursors_mut().set_single(end_pos);
    editor.move_down(false); // at last line
    assert_eq!(editor.cursors().primary().head, end_pos);

    // Tab cycling and management
    let tab2 = editor.new_tab();
    assert_eq!(tab2, 1);
    assert_eq!(editor.tabs().len(), 2);
    assert_eq!(editor.active_tab_idx(), 1);

    editor.next_tab();
    assert_eq!(editor.active_tab_idx(), 0);
    editor.prev_tab();
    assert_eq!(editor.active_tab_idx(), 1);

    assert!(editor.switch_tab(0));
    assert!(!editor.switch_tab(999));

    assert!(editor.close_tab(1));
    assert!(!editor.close_tab(999));
    assert_eq!(editor.tabs().len(), 1);

    // Closing the only remaining tab auto-spawns a new clean tab
    assert!(editor.close_tab(0));
    assert_eq!(editor.tabs().len(), 1);
}

#[test]
fn test_open_or_load_distinct_paths_do_not_false_match() {
    let mut ed = Editor::new();
    let dir = std::env::temp_dir().join(format!("lume_distinct_test_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let p1 = dir.join("file_a.rs");
    let p2 = dir.join("file_b.rs");
    std::fs::write(&p1, "// a").unwrap();
    std::fs::write(&p2, "// b").unwrap();

    let idx1 = ed.open_or_load(&p1).unwrap();
    assert_eq!(idx1, 0);
    assert_eq!(ed.tabs().len(), 1);

    let idx2 = ed.open_or_load(&p2).unwrap();
    assert_eq!(idx2, 1);
    assert_eq!(ed.tabs().len(), 2);
    assert_eq!(ed.tabs()[0].file_path.as_deref(), Some(p1.as_path()));
    assert_eq!(ed.tabs()[1].file_path.as_deref(), Some(p2.as_path()));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_editor_crlf_line_clamping_and_move_end() {
    let mut ed = Editor::new();
    ed.insert_str("line one\r\nsecond line\r\nthird").unwrap();

    // Line 0: "line one" has 8 chars, followed by \r\n
    ed.set_cursor_from_line_col(0, 100, false);
    let (line, col) = ed.cursor_line_col();
    assert_eq!((line, col), (0, 8));

    ed.move_home(false);
    assert_eq!(ed.cursor_line_col(), (0, 0));
    ed.move_end(false);
    assert_eq!(ed.cursor_line_col(), (0, 8));

    // Line 1: "second line" has 11 chars, followed by \r\n
    ed.set_cursor_from_line_col(1, 100, false);
    let (line, col) = ed.cursor_line_col();
    assert_eq!((line, col), (1, 11));

    ed.move_home(false);
    assert_eq!(ed.cursor_line_col(), (1, 0));
    ed.move_end(false);
    assert_eq!(ed.cursor_line_col(), (1, 11));
}
