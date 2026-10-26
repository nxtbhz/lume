use crate::buffer::TextBuffer;
use crate::editor::Editor;
use crate::journal::Journal;

#[test]
fn test_editor_atomic_save_and_journal_cleanup() {
    let tmp_dir = std::env::temp_dir();
    let file_path = tmp_dir.join(format!("lume_test_save_{}.txt", std::process::id()));

    let mut ed = Editor::new();
    ed.set_file_path(Some(file_path.clone()));
    ed.insert_str("Atomic save content").unwrap();

    // While dirty, journal exists
    assert!(Journal::has_journal(&file_path));

    ed.save().unwrap();
    assert!(!ed.is_dirty());
    assert!(file_path.exists());
    // Clean save removes journal
    assert!(!Journal::has_journal(&file_path));

    let read_back = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(read_back, "Atomic save content");

    let _ = std::fs::remove_file(&file_path);
}

#[test]
fn test_editor_crash_recovery() {
    let tmp_dir = std::env::temp_dir();
    let file_path = tmp_dir.join(format!("lume_crash_test_{}.txt", std::process::id()));
    std::fs::write(&file_path, "original disk text").unwrap();

    // 1. User opens file and modifies it
    {
        let mut ed = Editor::open(&file_path).unwrap();
        ed.set_cursor_from_line_col(0, 18, false);
        ed.insert_str(" + unsaved edits").unwrap();
        // Journal records unsaved changes!
        assert!(Journal::has_journal(&file_path));
        // Simulate crash: ed is dropped without calling save()!
    }

    // 2. User re-opens file after crash
    let recovered_ed = Editor::open(&file_path).unwrap();
    assert!(recovered_ed.active_tab().recovered_from_journal);
    assert!(recovered_ed.is_dirty());
    let text = recovered_ed
        .buffer()
        .slice_to_cow(0, recovered_ed.buffer().len_bytes());
    assert_eq!(text, "original disk text + unsaved edits");

    // Clean up
    let _ = Journal::remove(&file_path);
    let _ = std::fs::remove_file(&file_path);
}

#[test]
fn test_editor_async_autosave() {
    let tmp_dir = std::env::temp_dir();
    let file_path = tmp_dir.join(format!("lume_test_autosave_{}.txt", std::process::id()));

    let mut ed = Editor::new();
    ed.set_file_path(Some(file_path.clone()));
    ed.autosave_controller_mut().config.debounce = std::time::Duration::from_millis(0);

    ed.insert_str("async autosave content").unwrap();
    assert!(ed.is_dirty());

    let triggered = ed.check_autosave();
    assert!(triggered);

    for _ in 0..50 {
        let _ = ed.poll_async_saves();
        if !ed.is_dirty() && file_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&file_path) {
                if content == "async autosave content" {
                    break;
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    assert!(!ed.is_dirty());
    assert!(file_path.exists());
    let read_back = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(read_back, "async autosave content");

    let _ = std::fs::remove_file(&file_path);
}

#[test]
fn test_async_save_preserves_journal_for_newer_edits() {
    let tmp_dir = std::env::temp_dir();
    let file_path = tmp_dir.join(format!(
        "lume_test_autosave_journal_race_{}.txt",
        std::process::id()
    ));
    std::fs::write(&file_path, "base").unwrap();

    let mut ed = Editor::open(&file_path).unwrap();
    ed.set_cursor_from_line_col(0, 4, false);
    ed.insert_str(" + edit1").unwrap();
    assert_eq!(ed.revision(), 1);
    assert!(Journal::has_journal(&file_path));

    // Trigger async save at revision 1
    ed.save_async().unwrap();

    // User continues typing (revision 2) before/while async save finishes
    ed.insert_str(" + edit2").unwrap();
    assert_eq!(ed.revision(), 2);

    // Wait for async save of revision 1 to complete
    for _ in 0..50 {
        let events = ed.poll_async_saves();
        if !events.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // Tab must still be dirty and journal must NOT be deleted because revision 2 > save_revision 1
    assert!(ed.is_dirty());
    assert!(
        Journal::has_journal(&file_path),
        "Journal for newer edits must not be deleted by older background save"
    );

    // Recovering from journal must include edit2
    let recovered = Editor::open(&file_path).unwrap();
    assert_eq!(
        recovered
            .buffer()
            .slice_to_cow(0, recovered.buffer().len_bytes()),
        "base + edit1 + edit2"
    );

    let _ = Journal::remove(&file_path);
    let _ = std::fs::remove_file(&file_path);
}

#[test]
fn test_async_save_failure_keeps_dirty_and_reports_error() {
    // Point to a path inside a regular file (so creating parent directory fails with I/O error)
    let tmp_dir = std::env::temp_dir();
    let blocker_file = tmp_dir.join(format!("lume_blocker_file_{}", std::process::id()));
    std::fs::write(&blocker_file, "not a directory").unwrap();
    let invalid_target = blocker_file.join("subdir").join("cannot_write.txt");

    let mut ed = Editor::new();
    ed.set_file_path(Some(invalid_target.clone()));
    ed.insert_str("important unsaved data").unwrap();
    assert!(ed.is_dirty());

    ed.save_async().unwrap();
    // Immediately after save_async, buffer must remain dirty
    assert!(ed.is_dirty());

    let mut received_err = false;
    for _ in 0..50 {
        let events = ed.poll_async_saves();
        if let Some(ev) = events.first() {
            assert!(ev.result.is_err());
            received_err = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    assert!(received_err, "Expected async save I/O error event");
    assert!(ed.is_dirty(), "Tab must remain dirty when async save fails");

    let _ = Journal::remove(&invalid_target);
    let _ = std::fs::remove_file(&blocker_file);
}

#[test]
fn test_close_dirty_tab_preserves_journal() {
    let tmp_dir = std::env::temp_dir();
    let file_path = tmp_dir.join(format!("lume_close_dirty_tab_{}.txt", std::process::id()));
    std::fs::write(&file_path, "initial content").unwrap();

    let mut ed = Editor::open(&file_path).unwrap();
    ed.insert_str("unsaved prefix ").unwrap();
    assert!(ed.is_dirty());
    assert!(Journal::has_journal(&file_path));

    // Closing a dirty tab must NOT delete the crash recovery journal
    ed.close_tab(0);
    assert!(Journal::has_journal(&file_path));

    // Re-opening recovers the unsaved edits
    let recovered = Editor::open(&file_path).unwrap();
    assert_eq!(
        recovered
            .buffer()
            .slice_to_cow(0, recovered.buffer().len_bytes()),
        "unsaved prefix initial content"
    );

    let _ = Journal::remove(&file_path);
    let _ = std::fs::remove_file(&file_path);
}
