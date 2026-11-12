use crate::diff::LineDiffStatus;
use crate::editor::Editor;
use std::fs;
use std::time::SystemTime;

#[test]
fn test_tab_diff_markers_lifecycle() {
    let unique_id = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_file = std::env::temp_dir().join(format!("lume_tab_diff_{}.txt", unique_id));
    fs::write(&temp_file, "line 1\nline 2\nline 3\n").unwrap();

    let mut ed = Editor::open(&temp_file).expect("should open file");
    assert!(!ed.is_dirty());
    {
        let tab = ed.active_tab_mut();
        let markers = tab.diff_markers();
        assert!(markers.is_none());
    }

    // Insert a new line at line 2
    ed.move_down(false);
    let _ = ed.insert_char('\n');
    let _ = ed.insert_str("new line");
    assert!(ed.is_dirty());

    {
        let tab = ed.active_tab_mut();
        let markers = tab
            .diff_markers()
            .expect("dirty tab should produce diff markers");
        assert!(markers
            .iter()
            .any(|&status| status == LineDiffStatus::Added || status == LineDiffStatus::Modified));
    }

    // Save tab and verify diffs are cleared
    ed.save().expect("save should succeed");
    assert!(!ed.is_dirty());
    {
        let tab = ed.active_tab_mut();
        let markers = tab.diff_markers();
        assert!(markers.is_none());
    }

    let _ = fs::remove_file(&temp_file);
}

#[test]
fn test_editor_undo_then_edit_crash_recovery_and_diff_baseline() {
    use crate::buffer::TextBuffer;
    use crate::journal::Journal;

    let dir = std::env::temp_dir().join(format!(
        "lume_test_editor_undo_recovery_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let file_path = dir.join("main.rs");
    fs::write(&file_path, "line 1\nline 2\n").unwrap();

    let _ = Journal::remove(&file_path);
    {
        let mut ed = Editor::open(&file_path).unwrap();
        ed.move_end(false);
        ed.insert_char('!').unwrap();
        assert!(ed.undo());
        ed.insert_char('?').unwrap();
        // Drop `ed` without saving to simulate crash after undo + edit
    }

    let mut recovered_ed = Editor::open(&file_path).unwrap();
    assert!(recovered_ed.active_tab().pending_recovery.is_some());
    assert!(recovered_ed.recover_tab_journal(0));
    assert!(recovered_ed.is_dirty());
    assert_eq!(
        recovered_ed
            .buffer()
            .slice_to_cow(0, recovered_ed.buffer().len_bytes()),
        "line 1?\nline 2\n"
    );
    let markers = recovered_ed
        .active_tab_mut()
        .diff_markers()
        .expect("recovered dirty tab should produce diff markers against disk baseline");
    assert_eq!(
        markers.as_slice(),
        &[
            LineDiffStatus::Modified,
            LineDiffStatus::Unchanged,
            LineDiffStatus::Unchanged
        ]
    );

    let _ = Journal::remove(&file_path);
    let _ = fs::remove_dir_all(&dir);
}
