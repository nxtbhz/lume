use crate::buffer::TextBuffer;
use crate::cursor::Cursor;
use crate::editor::Editor;

#[test]
fn test_editor_clipboard_copy_selection_and_paste() {
    let mut ed = Editor::from_str("hello world");
    ed.cursors_mut().primary_mut().anchor = 6;
    ed.cursors_mut().primary_mut().head = 11;

    let copied = ed.copy_selection();
    assert_eq!(copied.as_deref(), Some("world"));
    assert_eq!(ed.clipboard(), Some("world"));
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "hello world"
    );

    ed.cursors_mut().set_single(0);
    let pasted = ed.paste();
    assert!(pasted);
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "worldhello world"
    );
}

#[test]
fn test_editor_clipboard_cut_selection_and_undo() {
    let mut ed = Editor::from_str("hello beautiful world");
    ed.cursors_mut().primary_mut().anchor = 5;
    ed.cursors_mut().primary_mut().head = 15;

    let cut = ed.cut_selection();
    assert_eq!(cut.as_deref(), Some(" beautiful"));
    assert_eq!(ed.clipboard(), Some(" beautiful"));
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "hello world"
    );
    assert_eq!(ed.cursors().primary().head, 5);

    assert!(ed.undo());
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "hello beautiful world"
    );
    assert_eq!(ed.cursors().primary().range(), 5..15);
}

#[test]
fn test_editor_clipboard_copy_and_cut_line_when_no_selection() {
    let mut ed = Editor::from_str("line 1\nline 2\nline 3");
    ed.cursors_mut().set_single(9);

    let copied = ed.copy_selection();
    assert_eq!(copied.as_deref(), Some("line 2\n"));
    assert_eq!(ed.clipboard(), Some("line 2\n"));
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "line 1\nline 2\nline 3"
    );

    let cut = ed.cut_selection();
    assert_eq!(cut.as_deref(), Some("line 2\n"));
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "line 1\nline 3"
    );

    assert!(ed.undo());
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "line 1\nline 2\nline 3"
    );
}

#[test]
fn test_editor_clipboard_cross_tab_sharing() {
    let mut ed = Editor::from_str("tab 1 content");
    ed.cursors_mut().primary_mut().anchor = 0;
    ed.cursors_mut().primary_mut().head = 5;
    ed.copy_selection();
    assert_eq!(ed.clipboard(), Some("tab 1"));

    let tab2_idx = ed.new_tab();
    assert_eq!(tab2_idx, 1);
    assert_eq!(ed.clipboard(), Some("tab 1"));

    let pasted = ed.paste();
    assert!(pasted);
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "tab 1"
    );

    ed.switch_tab(0);
    ed.close_tab(0);
    assert_eq!(ed.clipboard(), Some("tab 1"));
}

#[test]
fn test_editor_clipboard_empty_buffer_and_multibyte() {
    let mut ed = Editor::new();
    assert_eq!(ed.copy_selection(), None);
    assert_eq!(ed.cut_selection(), None);
    assert!(!ed.paste());

    ed.insert_str("Ação de copiar 🚀").unwrap();
    ed.cursors_mut().primary_mut().anchor = 0;
    ed.cursors_mut().primary_mut().head = ed.buffer().len_bytes();
    let copied = ed.copy_selection();
    assert_eq!(copied.as_deref(), Some("Ação de copiar 🚀"));

    ed.cursors_mut().set_single(0);
    ed.paste();
    assert!(ed
        .buffer()
        .slice_to_cow(0, ed.buffer().len_bytes())
        .starts_with("Ação de copiar 🚀"));
}

#[test]
fn test_editor_clipboard_multi_cursor() {
    let mut ed = Editor::from_str("foo\nbar\nbaz");
    ed.cursors_mut().set_cursors(vec![
        Cursor::with_anchor(0, 3),
        Cursor::with_anchor(4, 7),
    ]);
    let copied = ed.copy_selection();
    assert_eq!(copied.as_deref(), Some("foo\nbar"));

    let cut = ed.cut_selection();
    assert_eq!(cut.as_deref(), Some("foo\nbar"));
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "\n\nbaz"
    );

    assert!(ed.undo());
    assert_eq!(
        ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
        "foo\nbar\nbaz"
    );
}
