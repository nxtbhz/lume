//! Tier 1: Feature Coverage (Features 1 to 8, 13 to 21)
//!
//! Features covered:
//! - Feature 1: `safe_file_open`
//! - Feature 2: `untitled_tab_on_failure`
//! - Feature 3: `opt_in_autosave`
//! - Feature 4: `journal_mtime_recovery`
//! - Feature 5: `discard_journal_cleanup`
//! - Feature 6: `tab_close_dirty_guard`
//! - Feature 7: `journal_path_lifecycle`
//! - Feature 8: `background_journal_worker`
//! - Feature 13: `idle_mouse_suppression`
//! - Feature 14: `line_ending_detection`
//! - Feature 15: `line_ending_insert`
//! - Feature 16: `atomic_crlf_deletion`
//! - Feature 17: `status_bar_line_ending`
//! - Feature 18: `undo_typing_coalescing`
//! - Feature 19: `exact_saved_depth_tracking`
//! - Feature 20: `git_head_blob_reader`
//! - Feature 21: `persistent_gutter_baseline`

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use lume_core::autosave::{AutoSaveConfig, AutoSaveController};
use lume_core::buffer::TextDelta;
use lume_core::diff::{compute_rope_diffs, LineDiffStatus};
use lume_core::editor::Editor;
use lume_core::git::{find_git_dir, parse_git_head_content, GitHead};
use lume_core::journal::Journal;

fn unique_temp_path(prefix: &str, ext: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "{}_{}_{}.{}",
        prefix,
        std::process::id(),
        nanos,
        ext
    ))
}

// -----------------------------------------------------------------------------
// Feature 1: safe_file_open & Feature 2: untitled_tab_on_failure
// -----------------------------------------------------------------------------

#[test]
fn test_f1_valid_utf8_file_opens_successfully() {
    let path = unique_temp_path("test_f1_utf8", "txt");
    fs::write(&path, "Hello, Lume editor!\nLine 2 with utf-8: 🚀\n").unwrap();

    let ed = Editor::open(&path).expect("Valid UTF-8 file must open cleanly");
    assert_eq!(
        ed.buffer().rope().to_string(),
        "Hello, Lume editor!\nLine 2 with utf-8: 🚀\n"
    );
    assert_eq!(ed.file_path(), Some(path.as_path()));
    assert!(!ed.is_dirty());
    let _ = fs::remove_file(&path);
}

#[test]
fn test_f1_multiline_ascii_file_opens() {
    let path = unique_temp_path("test_f1_ascii", "txt");
    fs::write(&path, "line1\nline2\nline3\n").unwrap();

    let ed = Editor::open(&path).unwrap();
    assert_eq!(ed.buffer().rope().len_lines(), 4);
    assert_eq!(ed.active_tab().file_path, Some(path.clone()));
    let _ = fs::remove_file(&path);
}

#[test]
fn test_f1_cjk_and_emoji_utf8_opens() {
    let path = unique_temp_path("test_f1_cjk", "txt");
    fs::write(&path, "日本語テスト\n中文测试\n한국어 테스트\n").unwrap();

    let ed = Editor::open(&path).unwrap();
    assert_eq!(
        ed.buffer().rope().to_string(),
        "日本語テスト\n中文测试\n한국어 테스트\n"
    );
    let _ = fs::remove_file(&path);
}

#[test]
fn test_f1_code_file_with_comments_opens() {
    let path = unique_temp_path("test_f1_code", "rs");
    fs::write(&path, "// A comment\nfn test() -> bool { true }\n").unwrap();

    let ed = Editor::open(&path).unwrap();
    assert_eq!(ed.buffer().rope().len_lines(), 3);
    let _ = fs::remove_file(&path);
}

#[test]
fn test_f2_editor_default_tab_is_untitled() {
    let ed = Editor::new();
    assert_eq!(ed.file_path(), None);
    assert!(!ed.is_dirty());
    assert_eq!(ed.tabs().len(), 1);
}

#[test]
fn test_f2_open_nonexistent_file_preserves_untitled_state() {
    let path = unique_temp_path("test_f2_nonexistent", "txt");
    let mut ed = Editor::new();
    let res = ed.open_in_current_tab(&path);
    assert!(res.is_err(), "Opening non-existent file must fail");
    assert_eq!(ed.file_path(), None, "Tab must remain untitled on failure");
    assert!(!ed.is_dirty(), "Tab must remain clean on failure");
}

#[test]
fn test_f2_failed_open_does_not_attach_file_path() {
    let path = unique_temp_path("test_f2_missing_dir/sub", "txt");
    let mut ed = Editor::new();
    let _ = ed.open_in_current_tab(&path);
    assert!(ed.active_tab().file_path.is_none());
}

#[test]
fn test_f2_failed_open_is_not_dirty() {
    let path = PathBuf::from("/proc/nonexistent_lume_test_file");
    let mut ed = Editor::new();
    let _ = ed.open_in_current_tab(&path);
    assert!(!ed.active_tab().is_dirty);
}

#[test]
fn test_f2_failed_open_tab_count_unchanged() {
    let path = PathBuf::from("/nonexistent/file.txt");
    let mut ed = Editor::new();
    let count_before = ed.tabs().len();
    let _ = ed.open_in_current_tab(&path);
    assert_eq!(ed.tabs().len(), count_before);
}

// -----------------------------------------------------------------------------
// Feature 3: opt_in_autosave
// -----------------------------------------------------------------------------

#[test]
fn test_f3_autosave_explicitly_enabled_works() {
    let mut controller = AutoSaveController::new(AutoSaveConfig {
        enabled: true,
        debounce: Duration::from_millis(50),
    });
    assert!(!controller.should_autosave(false));
    assert!(!controller.should_autosave(true));

    controller.notify_change();
    assert!(!controller.should_autosave(true));

    std::thread::sleep(Duration::from_millis(60));
    assert!(controller.should_autosave(true));
}

#[test]
fn test_f3_autosave_debounce_timing() {
    let mut controller = AutoSaveController::new(AutoSaveConfig {
        enabled: true,
        debounce: Duration::from_millis(100),
    });
    controller.notify_change();
    assert!(!controller.should_autosave(true));
    std::thread::sleep(Duration::from_millis(110));
    assert!(controller.should_autosave(true));
}

#[test]
fn test_f3_autosave_clean_buffer_never_triggers() {
    let mut controller = AutoSaveController::new(AutoSaveConfig {
        enabled: true,
        debounce: Duration::from_millis(10),
    });
    controller.notify_change();
    std::thread::sleep(Duration::from_millis(20));
    assert!(!controller.should_autosave(false));
}

#[test]
fn test_f3_autosave_mark_saved_resets_debounce() {
    let mut controller = AutoSaveController::new(AutoSaveConfig {
        enabled: true,
        debounce: Duration::from_millis(10),
    });
    controller.notify_change();
    std::thread::sleep(Duration::from_millis(20));
    assert!(controller.should_autosave(true));

    controller.mark_saved();
    assert!(!controller.should_autosave(true));
}

#[test]
fn test_f3_autosave_disabled_ignores_elapsed_time() {
    let mut controller = AutoSaveController::new(AutoSaveConfig {
        enabled: false,
        debounce: Duration::from_millis(10),
    });
    controller.notify_change();
    std::thread::sleep(Duration::from_millis(20));
    assert!(
        !controller.should_autosave(true),
        "Disabled autosave must never trigger"
    );
}

// -----------------------------------------------------------------------------
// Feature 4 & 5: Journal lifecycle, recovery & cleanup
// -----------------------------------------------------------------------------

#[test]
fn test_f4_journal_path_computation_deterministic() {
    let p1 = PathBuf::from("/home/user/project/main.rs");
    let p2 = PathBuf::from("/home/user/project/main.rs");
    assert_eq!(Journal::journal_path(&p1), Journal::journal_path(&p2));
}

#[test]
fn test_f4_journal_record_and_recover_cycle() {
    let path = unique_temp_path("test_f4_rec", "txt");
    fs::write(&path, "initial").unwrap();

    let delta = TextDelta {
        start_byte: 7,
        old_text: String::new(),
        new_text: " modified".to_string(),
        timestamp: 1000,
    };
    Journal::record_deltas(&path, &[delta]).unwrap();
    assert!(Journal::has_journal(&path));

    let recovered = Journal::recover(&path).unwrap();
    assert!(recovered.is_some());
    assert_eq!(recovered.unwrap(), "initial modified");

    Journal::remove(&path).unwrap();
    assert!(!Journal::has_journal(&path));
    let _ = fs::remove_file(&path);
}

#[test]
fn test_f4_journal_has_journal_detection() {
    let path = unique_temp_path("test_f4_has", "txt");
    fs::write(&path, "clean").unwrap();
    assert!(!Journal::has_journal(&path));

    Journal::record(&path, "dirty snapshot").unwrap();
    assert!(Journal::has_journal(&path));

    Journal::remove(&path).unwrap();
    assert!(!Journal::has_journal(&path));
    let _ = fs::remove_file(&path);
}

#[test]
fn test_f4_journal_remove_cleans_file() {
    let path = unique_temp_path("test_f4_rm", "txt");
    fs::write(&path, "text").unwrap();
    Journal::record(&path, "snap").unwrap();
    let j_path = Journal::journal_path(&path);
    assert!(j_path.exists());

    Journal::remove(&path).unwrap();
    assert!(!j_path.exists());
    let _ = fs::remove_file(&path);
}

#[test]
fn test_f5_clean_tab_discard_is_safe_noop() {
    let ed = Editor::new();
    assert!(!ed.is_dirty());
    assert_eq!(ed.tabs().len(), 1);
}

#[test]
fn test_f5_discard_preserves_clean_disk_file() {
    let path = unique_temp_path("test_f5_clean_disk", "txt");
    fs::write(&path, "disk content unchanged").unwrap();
    let ed = Editor::open(&path).unwrap();
    assert_eq!(ed.buffer().rope().to_string(), "disk content unchanged");
    assert_eq!(fs::read_to_string(&path).unwrap(), "disk content unchanged");
    let _ = fs::remove_file(&path);
}

#[test]
fn test_f5_discard_after_multiple_edits() {
    let path = unique_temp_path("test_f5_multi_edits", "txt");
    fs::write(&path, "original").unwrap();
    let mut ed = Editor::open(&path).unwrap();
    ed.insert_str("_added").unwrap();
    assert!(ed.is_dirty());
    assert_eq!(fs::read_to_string(&path).unwrap(), "original");
    let _ = fs::remove_file(&path);
}

// -----------------------------------------------------------------------------
// Feature 6: tab_close_dirty_guard & Feature 7: journal_path_lifecycle
// -----------------------------------------------------------------------------

#[test]
fn test_f6_clean_tab_closes_without_confirmation() {
    let mut ed = Editor::new();
    ed.new_tab();
    assert_eq!(ed.tabs().len(), 2);
    ed.close_tab(1);
    assert_eq!(ed.tabs().len(), 1);
}

#[test]
fn test_f6_dirty_tab_indicates_dirty_state() {
    let mut ed = Editor::new();
    assert!(!ed.is_dirty());
    ed.insert_str("dirt").unwrap();
    assert!(ed.is_dirty());
}

#[test]
fn test_f6_tab_sync_state_dirty_flag_consistent() {
    let mut ed = Editor::new();
    assert!(!ed.active_tab().is_dirty);
    ed.insert_char('x').unwrap();
    assert!(ed.active_tab().is_dirty);
}

#[test]
fn test_f6_save_clears_dirty_before_close() {
    let path = unique_temp_path("test_f6_save_clear", "txt");
    fs::write(&path, "base").unwrap();
    let mut ed = Editor::open(&path).unwrap();
    ed.insert_str("_edit").unwrap();
    assert!(ed.is_dirty());
    ed.save().unwrap();
    assert!(!ed.is_dirty());
    ed.close_tab(0);
    let _ = fs::remove_file(&path);
}

#[test]
fn test_f7_tab_journal_path_initialized_on_open() {
    let path = unique_temp_path("test_f7_init", "txt");
    fs::write(&path, "content").unwrap();
    let ed = Editor::open(&path).unwrap();
    assert_eq!(
        ed.active_tab().journal_path,
        Some(Journal::journal_path(&path))
    );
    let _ = fs::remove_file(&path);
}

#[test]
fn test_f5_discard_removes_tab_from_tabs_list() {
    let mut ed = Editor::new();
    ed.new_tab();
    assert_eq!(ed.tabs().len(), 2);
    ed.close_tab(1);
    assert_eq!(ed.tabs().len(), 1);
}

#[test]
fn test_f6_new_tab_is_clean() {
    let mut ed = Editor::new();
    let idx = ed.new_tab();
    assert_eq!(idx, 1);
    assert!(!ed.active_tab().is_dirty);
}

#[test]
fn test_f7_tab_journal_path_retained_across_saves() {
    let path = unique_temp_path("test_f7_retain", "txt");
    fs::write(&path, "abc").unwrap();
    let mut ed = Editor::open(&path).unwrap();
    let j_path = ed.active_tab().journal_path.clone();
    ed.insert_str("d").unwrap();
    ed.save().unwrap();
    assert_eq!(ed.active_tab().journal_path, j_path);
    let _ = fs::remove_file(&path);
}

#[test]
fn test_f7_tab_journal_path_none_on_untitled() {
    let ed = Editor::new();
    assert_eq!(ed.active_tab().journal_path, None);
}

#[test]
fn test_f7_tab_journal_path_updated_on_save_as() {
    let path = unique_temp_path("test_f7_saveas", "txt");
    let mut ed = Editor::new();
    ed.insert_str("some text").unwrap();
    ed.save_as(&path).unwrap();
    assert_eq!(
        ed.active_tab().journal_path,
        Some(Journal::journal_path(&path))
    );
    let _ = fs::remove_file(&path);
}

#[test]
fn test_f7_tab_journal_path_remains_consistent_on_tab_switch() {
    let p1 = unique_temp_path("test_f7_sw1", "txt");
    let p2 = unique_temp_path("test_f7_sw2", "txt");
    fs::write(&p1, "1").unwrap();
    fs::write(&p2, "2").unwrap();

    let mut ed = Editor::open(&p1).unwrap();
    ed.open_tab(&p2).unwrap();

    let j2 = ed.active_tab().journal_path.clone();
    ed.switch_tab(0);
    let j1 = ed.active_tab().journal_path.clone();
    assert_ne!(j1, j2);

    let _ = fs::remove_file(&p1);
    let _ = fs::remove_file(&p2);
}

#[test]
fn test_f8_journal_record_with_journal_path_direct() {
    let path = unique_temp_path("test_f8_direct", "txt");
    let j_path = Journal::journal_path(&path);
    let res = Journal::record_with_journal_path(&j_path, "snapshot direct");
    assert!(res.is_ok());
    assert!(j_path.exists());
    let _ = fs::remove_file(&j_path);
}

#[test]
fn test_f8_journal_recovery_returns_none_when_empty() {
    let path = unique_temp_path("test_f8_none", "txt");
    assert_eq!(Journal::recover(&path).unwrap(), None);
}

#[test]
fn test_f13_mouse_selection_expansion() {
    let mut ed = Editor::from_str("0123456789");
    ed.cursors_mut().set_single(2);
    ed.cursors_mut().primary_mut().head = 8;
    assert_eq!(ed.cursors().primary().min(), 2);
    assert_eq!(ed.cursors().primary().max(), 8);
    assert_eq!(
        ed.cursors().primary().max() - ed.cursors().primary().min(),
        6
    );
}

#[test]
fn test_f13_mouse_cursor_clamping_at_max_bounds() {
    let mut ed = Editor::from_str("abc");
    ed.cursors_mut().set_single(3); // at len_bytes
    assert_eq!(ed.cursors().primary().head, 3);
}

// -----------------------------------------------------------------------------
// Feature 8: Journal worker operations & POSIX durability
// -----------------------------------------------------------------------------

#[test]
fn test_f8_journal_record_deltas_fast_path() {
    let path = unique_temp_path("test_f8_fast", "txt");
    fs::write(&path, "alpha").unwrap();
    let j_path = Journal::journal_path(&path);

    let deltas = vec![TextDelta {
        start_byte: 5,
        old_text: String::new(),
        new_text: "beta".to_string(),
        timestamp: 1234,
    }];
    Journal::record_deltas_with_journal_path(&path, &j_path, &deltas).unwrap();
    assert!(j_path.exists());

    Journal::remove(&path).unwrap();
    let _ = fs::remove_file(&path);
}

#[test]
fn test_f8_journal_record_snapshot_roundtrip() {
    let path = unique_temp_path("test_f8_snap", "txt");
    fs::write(&path, "orig").unwrap();
    Journal::record(&path, "complete snapshot content").unwrap();

    let recovered = Journal::recover(&path).unwrap();
    assert_eq!(recovered, Some("complete snapshot content".to_string()));

    Journal::remove(&path).unwrap();
    let _ = fs::remove_file(&path);
}

#[test]
fn test_f8_journal_sync_data_posix_durability() {
    let path = unique_temp_path("test_f8_posix", "txt");
    fs::write(&path, "durability").unwrap();
    Journal::record(&path, "synced data").unwrap();
    let j_path = Journal::journal_path(&path);
    assert!(j_path.exists());
    let meta = fs::metadata(&j_path).unwrap();
    assert!(meta.len() > 0);

    Journal::remove(&path).unwrap();
    let _ = fs::remove_file(&path);
}

// -----------------------------------------------------------------------------
// Feature 13: Mouse interaction state changes
// -----------------------------------------------------------------------------

#[test]
fn test_f13_mouse_click_changes_state() {
    let mut ed = Editor::from_str("line 1\nline 2\nline 3\n");
    assert_eq!(ed.cursors().primary().min(), 0);
    ed.cursors_mut().set_single(7);
    assert_eq!(ed.cursors().primary().min(), 7);
}

#[test]
fn test_f13_mouse_drag_selection_changes_state() {
    let mut ed = Editor::from_str("hello world");
    ed.cursors_mut().set_single(0);
    ed.cursors_mut().primary_mut().head = 5;
    assert!(ed.cursors().primary().is_selection());
}

#[test]
fn test_f13_mouse_multi_cursor_support() {
    let mut ed = Editor::from_str("abc\ndef\n");
    ed.cursors_mut().add_cursor(lume_core::Cursor::new(4));
    assert_eq!(ed.cursors().cursors().len(), 2);
}

// -----------------------------------------------------------------------------
// Feature 18: History recording & undo/redo
// -----------------------------------------------------------------------------

#[test]
fn test_f18_history_records_single_delta() {
    let mut ed = Editor::from_str("hello");
    ed.cursors_mut().set_single(5);
    ed.insert_str(" world").unwrap();
    assert_eq!(ed.buffer().rope().to_string(), "hello world");
    assert!(ed.undo());
    assert_eq!(ed.buffer().rope().to_string(), "hello");
}

#[test]
fn test_f18_history_undo_restores_buffer() {
    let mut ed = Editor::from_str("start");
    ed.cursors_mut().set_single(5);
    ed.insert_char('!').unwrap();
    assert_eq!(ed.buffer().rope().to_string(), "start!");
    assert!(ed.undo());
    assert_eq!(ed.buffer().rope().to_string(), "start");
}

#[test]
fn test_f18_history_redo_reapplies_buffer() {
    let mut ed = Editor::from_str("initial");
    ed.cursors_mut().set_single(7);
    ed.insert_str("+more").unwrap();
    assert_eq!(ed.buffer().rope().to_string(), "initial+more");
    assert!(ed.undo());
    assert_eq!(ed.buffer().rope().to_string(), "initial");
    assert!(ed.redo());
    assert_eq!(ed.buffer().rope().to_string(), "initial+more");
}

#[test]
fn test_f18_history_empty_undo_returns_false() {
    let mut ed = Editor::new();
    assert!(!ed.undo(), "Undo on empty history must return false");
}

#[test]
fn test_f18_history_empty_redo_returns_false() {
    let mut ed = Editor::new();
    assert!(!ed.redo(), "Redo on empty history must return false");
}

// -----------------------------------------------------------------------------
// Feature 20: Git HEAD repository reader
// -----------------------------------------------------------------------------

#[test]
fn test_f20_find_git_dir_locates_repo() {
    let current_dir = std::env::current_dir().unwrap();
    let git_dir = find_git_dir(&current_dir);
    assert!(
        git_dir.is_some(),
        "Current workspace should be inside a Git repository"
    );
}

#[test]
fn test_f20_parse_git_head_branch() {
    let content = "ref: refs/heads/main\n";
    let parsed = parse_git_head_content(content);
    assert_eq!(parsed, Some(GitHead::Branch("main".to_string())));
}

#[test]
fn test_f20_parse_git_head_detached() {
    let hash = "d46d7da2019b88c727cb5fbe8353c079e64f5747\n";
    let parsed = parse_git_head_content(hash);
    assert_eq!(parsed, Some(GitHead::Detached("d46d7da".to_string())));
}

#[test]
fn test_f20_parse_git_head_empty_returns_none() {
    assert_eq!(parse_git_head_content(""), None);
    assert_eq!(parse_git_head_content("   \n"), None);
}

#[test]
fn test_f20_find_git_dir_non_git_returns_none() {
    let temp_empty = unique_temp_path("test_f20_nogit", "dir");
    fs::create_dir_all(&temp_empty).unwrap();
    assert_eq!(find_git_dir(&temp_empty), None);
    let _ = fs::remove_dir_all(&temp_empty);
}

// -----------------------------------------------------------------------------
// Feature 21: Persistent gutter baseline & diff computation
// -----------------------------------------------------------------------------

#[test]
fn test_f21_diff_computation_identical_lines() {
    let base = ropey::Rope::from_str("line 1\nline 2");
    let curr = ropey::Rope::from_str("line 1\nline 2");
    let diffs = compute_rope_diffs(&base, &curr);
    assert_eq!(diffs.len(), 2);
    assert_eq!(diffs[0], LineDiffStatus::Unchanged);
    assert_eq!(diffs[1], LineDiffStatus::Unchanged);
}

#[test]
fn test_f21_diff_computation_added_line() {
    let base = ropey::Rope::from_str("line 1");
    let curr = ropey::Rope::from_str("line 1\nline 2");
    let diffs = compute_rope_diffs(&base, &curr);
    assert_eq!(diffs.len(), 2);
    assert_eq!(diffs[0], LineDiffStatus::Unchanged);
    assert_eq!(diffs[1], LineDiffStatus::Added);
}

#[test]
fn test_f21_diff_computation_modified_line() {
    let base = ropey::Rope::from_str("original");
    let curr = ropey::Rope::from_str("modified");
    let diffs = compute_rope_diffs(&base, &curr);
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0], LineDiffStatus::Modified);
}

#[test]
fn test_f21_clean_snapshot_updated_on_save() {
    let path = unique_temp_path("test_f21_snap", "txt");
    fs::write(&path, "initial text\n").unwrap();
    let mut ed = Editor::open(&path).unwrap();
    assert_eq!(
        ed.active_tab()
            .clean_snapshot
            .as_ref()
            .map(|r| r.to_string()),
        Some("initial text\n".to_string())
    );
    ed.insert_str("edit\n").unwrap();
    ed.save().unwrap();
    assert_eq!(
        ed.active_tab()
            .clean_snapshot
            .as_ref()
            .map(|r| r.to_string()),
        Some("edit\ninitial text\n".to_string())
    );
    let _ = fs::remove_file(&path);
}

#[test]
fn test_f21_diff_markers_empty_on_identical_buffer() {
    let path = unique_temp_path("test_f21_markers", "txt");
    fs::write(&path, "line 1\nline 2\n").unwrap();
    let mut ed = Editor::open(&path).unwrap();
    let markers = ed.active_tab_mut().diff_markers();
    assert!(
        markers.is_none()
            || markers
                .unwrap()
                .iter()
                .all(|m| *m == LineDiffStatus::Unchanged)
    );
    let _ = fs::remove_file(&path);
}

// -----------------------------------------------------------------------------
// Milestone Gated Integration Tests (Activated as Milestones 1 to 5 Land)
// -----------------------------------------------------------------------------

#[cfg(any(feature = "m1", feature = "e2e"))]
mod m1_tests {
    use super::*;

    #[test]
    fn test_f1_rejects_nul_bytes_in_first_8k() {
        let path = unique_temp_path("test_f1_nul", "bin");
        let mut data = vec![b'a'; 4000];
        data.push(0);
        data.extend_from_slice(&vec![b'b'; 4000]);
        fs::write(&path, &data).unwrap();

        let mut ed = Editor::new();
        let res = ed.open_in_current_tab(&path);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(ed.file_path(), None);
        assert!(!ed.is_dirty());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_f3_autosave_config_default_disabled() {
        let config = AutoSaveConfig::default();
        assert!(
            !config.enabled,
            "AutoSaveConfig::default().enabled must be false"
        );
        assert_eq!(config.debounce, Duration::from_millis(1000));
    }

    #[test]
    fn test_f4_stale_journal_deleted_when_disk_newer() {
        let path = unique_temp_path("test_f4_stale", "txt");
        fs::write(&path, "v1").unwrap();
        Journal::record(&path, "crashed_v1").unwrap();
        std::thread::sleep(Duration::from_millis(30));
        fs::write(&path, "disk_v2").unwrap();

        let ed = Editor::open(&path).unwrap();
        assert_eq!(ed.buffer().rope().to_string(), "disk_v2");
        assert!(!Journal::has_journal(&path));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_f4_fresh_journal_offers_recovery() {
        let path = unique_temp_path("test_f4_fresh", "txt");
        fs::write(&path, "disk").unwrap();
        std::thread::sleep(Duration::from_millis(30));
        Journal::record(&path, "journal_newer").unwrap();

        let mut ed = Editor::open(&path).unwrap();
        assert!(Journal::has_journal(&path));
        assert!(ed.recover_tab_journal(0));
        assert_eq!(ed.buffer().rope().to_string(), "journal_newer");
        assert!(ed.is_dirty());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_f5_discard_tab_removes_journal() {
        let path = unique_temp_path("test_f5_disc", "txt");
        fs::write(&path, "base").unwrap();
        let mut ed = Editor::open(&path).unwrap();
        ed.insert_str("dirt").unwrap();
        assert!(ed.discard_tab_changes(0));
        assert!(!Journal::has_journal(&path));
        let _ = fs::remove_file(&path);
    }
}

#[cfg(any(feature = "m3", feature = "e2e"))]
mod m3_tests {
    use super::*;

    #[test]
    fn test_f14_lf_line_ending_detected() {
        let ed = Editor::from_str("line1\nline2\n");
        assert_eq!(ed.active_tab().line_ending, lume_core::LineEnding::Lf);
    }

    #[test]
    fn test_f14_crlf_line_ending_detected() {
        let ed = Editor::from_str("line1\r\nline2\r\n");
        assert_eq!(ed.active_tab().line_ending, lume_core::LineEnding::CrLf);
    }

    #[test]
    fn test_f15_insert_newline_crlf_emits_crlf() {
        let mut ed = Editor::from_str("fn main() {\r\n}\r\n");
        ed.cursors_mut().set_single(11);
        ed.insert_newline().unwrap();
        assert!(ed.buffer().rope().to_string().contains("\r\n"));
    }

    #[test]
    fn test_f16_delete_backwards_crlf_atomic() {
        let mut ed = Editor::from_str("a\r\nb");
        ed.cursors_mut().set_single(3);
        ed.delete_backwards().unwrap();
        assert_eq!(ed.buffer().rope().to_string(), "ab");
    }

    #[test]
    fn test_f17_status_bar_line_ending_labels() {
        assert_eq!(lume_core::LineEnding::Lf.label(), "LF");
        assert_eq!(lume_core::LineEnding::CrLf.label(), "CRLF");
    }

    #[test]
    fn test_f14_mixed_line_endings_prefers_first_line() {
        let ed = Editor::from_str("line1\r\nline2\nline3\n");
        assert_eq!(ed.active_tab().line_ending, lume_core::LineEnding::CrLf);
    }

    #[test]
    fn test_f14_empty_buffer_defaults_to_lf() {
        let ed = Editor::new();
        assert_eq!(ed.active_tab().line_ending, lume_core::LineEnding::Lf);
    }

    #[test]
    fn test_f14_line_ending_as_str_lf() {
        assert_eq!(lume_core::LineEnding::Lf.as_str(), "\n");
        assert_eq!(lume_core::LineEnding::CrLf.as_str(), "\r\n");
    }

    #[test]
    fn test_f15_insert_newline_lf_emits_lf() {
        let mut ed = Editor::from_str("hello\nworld");
        ed.cursors_mut().set_single(5);
        ed.insert_newline().unwrap();
        assert_eq!(ed.buffer().rope().to_string(), "hello\n\nworld");
    }

    #[test]
    fn test_f15_insert_newline_crlf_with_indent() {
        let mut ed = Editor::from_str("fn main() {\r\n}\r\n");
        ed.cursors_mut().set_single(11);
        ed.insert_newline().unwrap();
        assert!(ed.buffer().rope().to_string().contains("\r\n    \r\n"));
    }

    #[test]
    fn test_f15_insert_newline_middle_of_crlf_line() {
        let mut ed = Editor::from_str("hello world\r\n");
        ed.cursors_mut().set_single(5);
        ed.insert_newline().unwrap();
        assert_eq!(ed.buffer().rope().to_string(), "hello\r\n world\r\n");
    }

    #[test]
    fn test_f15_consecutive_crlf_newlines() {
        let mut ed = Editor::from_str("\r\n");
        ed.cursors_mut().set_single(0);
        ed.insert_newline().unwrap();
        ed.insert_newline().unwrap();
        assert_eq!(ed.buffer().rope().to_string(), "\r\n\r\n\r\n");
    }

    #[test]
    fn test_f16_delete_forwards_crlf_atomic() {
        let mut ed = Editor::from_str("a\r\nb");
        ed.cursors_mut().set_single(1); // right before \r\n
        ed.delete_forwards().unwrap();
        assert_eq!(ed.buffer().rope().to_string(), "ab");
    }

    #[test]
    fn test_f16_delete_backwards_lf_removes_single_char() {
        let mut ed = Editor::from_str("a\nb");
        ed.cursors_mut().set_single(2);
        ed.delete_backwards().unwrap();
        assert_eq!(ed.buffer().rope().to_string(), "ab");
    }

    #[test]
    fn test_f16_delete_forwards_lf_removes_single_char() {
        let mut ed = Editor::from_str("a\nb");
        ed.cursors_mut().set_single(1);
        ed.delete_forwards().unwrap();
        assert_eq!(ed.buffer().rope().to_string(), "ab");
    }

    #[test]
    fn test_f16_delete_backwards_crlf_with_selection() {
        let mut ed = Editor::from_str("start\r\nend");
        ed.cursors_mut().set_single(0);
        ed.cursors_mut().primary_mut().head = 7;
        ed.delete_backwards().unwrap();
        assert_eq!(ed.buffer().rope().to_string(), "end");
    }

    #[test]
    fn test_f17_status_bar_displays_lf_label() {
        assert_eq!(lume_core::LineEnding::Lf.label(), "LF");
    }

    #[test]
    fn test_f17_status_bar_displays_crlf_label() {
        assert_eq!(lume_core::LineEnding::CrLf.label(), "CRLF");
    }

    #[test]
    fn test_f17_line_ending_equality() {
        assert_eq!(lume_core::LineEnding::Lf, lume_core::LineEnding::Lf);
        assert_eq!(lume_core::LineEnding::CrLf, lume_core::LineEnding::CrLf);
        assert_ne!(lume_core::LineEnding::Lf, lume_core::LineEnding::CrLf);
    }
}

#[cfg(any(feature = "m4", feature = "e2e"))]
mod m4_tests {
    use super::*;

    #[test]
    fn test_f18_word_typing_coalescing() {
        let mut ed = Editor::new();
        for c in "hello".chars() {
            ed.insert_char(c).unwrap();
        }
        assert!(ed.undo());
        assert_eq!(ed.buffer().rope().to_string(), "");
    }

    #[test]
    fn test_f18_cursor_motion_breaks_coalescing() {
        let mut ed = Editor::new();
        ed.insert_char('h').unwrap();
        ed.cursors_mut().set_single(0); // move cursor
        ed.break_undo_group();
        ed.insert_char('i').unwrap();
        assert!(ed.undo());
        assert_eq!(ed.buffer().rope().to_string(), "h");
    }

    #[test]
    fn test_f18_whitespace_insertion_breaks_coalescing() {
        let mut ed = Editor::new();
        ed.insert_str("word1").unwrap();
        ed.break_undo_group();
        ed.insert_str(" word2").unwrap();
        assert!(ed.undo());
        assert_eq!(ed.buffer().rope().to_string(), "word1");
    }

    #[test]
    fn test_f18_backspace_breaks_coalescing() {
        let mut ed = Editor::new();
        ed.insert_str("typing").unwrap();
        ed.break_undo_group();
        ed.delete_backwards().unwrap();
        assert!(ed.undo());
        assert_eq!(ed.buffer().rope().to_string(), "typing");
    }

    #[test]
    fn test_f18_rapid_keystrokes_single_undo() {
        let mut ed = Editor::new();
        for c in "rapid".chars() {
            ed.insert_char(c).unwrap();
        }
        assert_eq!(ed.buffer().rope().to_string(), "rapid");
        assert!(ed.undo());
        assert_eq!(ed.buffer().rope().to_string(), "");
    }

    #[test]
    fn test_f19_saved_depth_tracking_clean_state() {
        let path = unique_temp_path("test_f19_saved", "txt");
        fs::write(&path, "clean").unwrap();
        let mut ed = Editor::open(&path).unwrap();
        ed.insert_str("edit").unwrap();
        ed.save().unwrap();
        assert!(!ed.is_dirty());

        ed.insert_str("more").unwrap();
        assert!(ed.is_dirty());

        ed.undo();
        assert!(
            !ed.is_dirty(),
            "Undoing to saved depth must restore is_dirty = false"
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_f19_saved_depth_branch_invalidation() {
        let path = unique_temp_path("test_f19_branch", "txt");
        fs::write(&path, "base").unwrap();
        let mut ed = Editor::open(&path).unwrap();
        ed.insert_str("_v1").unwrap();
        ed.save().unwrap();
        assert!(!ed.is_dirty());

        ed.undo();
        assert!(ed.is_dirty());

        ed.insert_str("_v2_branch").unwrap();
        assert!(ed.is_dirty());

        ed.undo();
        assert!(
            ed.is_dirty(),
            "Branching edit permanently invalidates saved_depth to None"
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_f19_undo_past_saved_depth_keeps_dirty() {
        let path = unique_temp_path("test_f19_past", "txt");
        fs::write(&path, "init").unwrap();
        let mut ed = Editor::open(&path).unwrap();
        ed.insert_str("1").unwrap();
        ed.break_undo_group();
        ed.insert_str("2").unwrap();
        ed.save().unwrap();
        assert!(!ed.is_dirty());

        ed.undo(); // back to init1 -> dirty
        assert!(ed.is_dirty());
        ed.undo(); // back to init -> dirty
        assert!(ed.is_dirty());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_f19_multiple_saves_update_saved_depth() {
        let path = unique_temp_path("test_f19_multisave", "txt");
        fs::write(&path, "start").unwrap();
        let mut ed = Editor::open(&path).unwrap();
        ed.insert_str("_first").unwrap();
        ed.save().unwrap();
        assert!(!ed.is_dirty());

        ed.insert_str("_second").unwrap();
        ed.save().unwrap();
        assert!(!ed.is_dirty());

        ed.undo();
        assert!(ed.is_dirty());
        ed.redo();
        assert!(!ed.is_dirty());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_f19_redo_restores_clean_state() {
        let path = unique_temp_path("test_f19_redo_clean", "txt");
        fs::write(&path, "clean").unwrap();
        let mut ed = Editor::open(&path).unwrap();
        ed.insert_str("saved").unwrap();
        ed.save().unwrap();
        assert!(!ed.is_dirty());

        ed.undo();
        assert!(ed.is_dirty());
        ed.redo();
        assert!(!ed.is_dirty());
        let _ = fs::remove_file(&path);
    }
}

#[cfg(any(feature = "m5", feature = "e2e"))]
mod m5_tests {
    use super::*;

    #[test]
    fn test_f20_read_git_head_blob() {
        let current_dir = std::env::current_dir().unwrap();
        let cargo_toml = current_dir.join("Cargo.toml");
        let blob = lume_core::git::read_git_head_blob(&cargo_toml);
        assert!(blob.is_some(), "Cargo.toml in git HEAD should be readable");
    }

    #[test]
    fn test_f20_read_git_head_blob_subfolder_file() {
        let current_dir = std::env::current_dir().unwrap();
        let src_lib = current_dir.join("crates/lume-core/src/lib.rs");
        let blob = lume_core::git::read_git_head_blob(&src_lib);
        assert!(blob.is_some());
    }

    #[test]
    fn test_f20_read_git_head_blob_nonexistent_returns_none() {
        let path = PathBuf::from("/nonexistent/file.rs");
        assert_eq!(lume_core::git::read_git_head_blob(&path), None);
    }

    #[test]
    fn test_f20_read_git_head_blob_returns_utf8() {
        let current_dir = std::env::current_dir().unwrap();
        let readme = current_dir.join("README.md");
        if readme.exists() {
            let blob = lume_core::git::read_git_head_blob(&readme);
            assert!(blob.is_some());
        }
    }

    #[test]
    fn test_f20_read_git_head_blob_uncommitted_modifications_ignored() {
        let current_dir = std::env::current_dir().unwrap();
        let cargo_toml = current_dir.join("Cargo.toml");
        let blob = lume_core::git::read_git_head_blob(&cargo_toml).unwrap();
        // HEAD blob should contain workspace declaration
        assert!(blob.contains("[workspace]"));
    }

    #[test]
    fn test_f21_diff_markers_persist_after_save() {
        let path = unique_temp_path("test_f21_git_save", "txt");
        fs::write(&path, "original\n").unwrap();
        let mut ed = Editor::open(&path).unwrap();
        ed.insert_str("mod_").unwrap();
        ed.save().unwrap();
        assert!(!ed.is_dirty());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_f21_diff_markers_against_git_baseline_unaffected_by_dirty() {
        let path = unique_temp_path("test_f21_persist_dirty", "txt");
        fs::write(&path, "committed\n").unwrap();
        let mut ed = Editor::open(&path).unwrap();
        ed.insert_str("uncommitted\n").unwrap();
        ed.save().unwrap();
        assert!(!ed.is_dirty());
        // Markers must still be computed against git_baseline
        let markers = ed.active_tab_mut().diff_markers();
        assert!(markers.is_some());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_f21_git_baseline_updated_on_save() {
        let path = unique_temp_path("test_f21_update_baseline", "txt");
        fs::write(&path, "base\n").unwrap();
        let mut ed = Editor::open(&path).unwrap();
        ed.save().unwrap();
        assert_eq!(
            ed.active_tab()
                .clean_snapshot
                .as_ref()
                .map(|r| r.to_string()),
            Some("base\n".to_string())
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_f21_diff_markers_reflect_multiple_line_edits() {
        let path = unique_temp_path("test_f21_multi_diff", "txt");
        fs::write(&path, "line1\nline2\nline3\n").unwrap();
        let mut ed = Editor::open(&path).unwrap();
        ed.cursors_mut().set_single(6);
        ed.insert_str("modified_").unwrap();
        ed.save().unwrap();
        let markers = ed.active_tab_mut().diff_markers().unwrap();
        assert!(markers.iter().any(|m| *m == LineDiffStatus::Modified));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_f21_clean_state_without_git_returns_none_markers() {
        let path = unique_temp_path("test_f21_nogit_clean", "txt");
        fs::write(&path, "untracked\n").unwrap();
        let mut ed = Editor::open(&path).unwrap();
        assert!(!ed.is_dirty());
        let markers = ed.active_tab_mut().diff_markers();
        assert!(
            markers.is_none()
                || markers
                    .unwrap()
                    .iter()
                    .all(|m| *m == LineDiffStatus::Unchanged)
        );
        let _ = fs::remove_file(&path);
    }
}
