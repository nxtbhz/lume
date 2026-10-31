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
