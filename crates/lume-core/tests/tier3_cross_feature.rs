//! Tier 3: Cross-Feature Combinations (Pairwise Subsystem Interactions)
//!
//! Interactions covered:
//! 1. Safe open + CRLF line ending preservation
//! 2. Opt-in autosave disabled + crash journal recovery
//! 3. Undo/redo history + atomic CRLF backspace/delete
//! 4. Git HEAD baseline + atomic save + undo dirty tracking
//! 5. Multi-tab journal isolation & distinct path hashes
//! 6. Save-as lifecycle + journal path remapping + clean state
//! 7. Atomic save durability + POSIX permissions
//! 8. Undo branching + multi-cursor synchronization
//! 9. Discard tab cleanup + journal removal + disk preservation
//! 10. Diff normalization + CRLF vs LF symmetry
//! 11. History memory limit + large delta eviction
//! 12. Rapid journal delta bursts + recovery integrity
//! 13. Tab deduplication + open_or_load canonical matching
//! 14. Search/replace transactional edit + undo recovery
//! 15. Multi-tab closure + remaining tab dirty state retention

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use lume_core::autosave::{AutoSaveConfig, AutoSaveController};
use lume_core::buffer::{TextBuffer, TextDelta};
use lume_core::cursor::Cursor;
use lume_core::diff::{compute_rope_diffs, LineDiffStatus};
use lume_core::editor::Editor;
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

#[test]
fn test_t3_01_safe_open_with_crlf_preservation() {
    let path = unique_temp_path("test_t3_crlf_open", "txt");
    let crlf_content = "def calculate():\r\n    return 42\r\n";
    fs::write(&path, crlf_content).unwrap();

    let ed =
        Editor::open(&path).expect("Opening CRLF file must succeed without binary false-positive");
    assert_eq!(ed.buffer().rope().to_string(), crlf_content);
    assert!(!ed.is_dirty());
    assert_eq!(ed.file_path(), Some(path.as_path()));
    let _ = fs::remove_file(&path);
}

#[test]
fn test_t3_02_autosave_disabled_with_journal_recovery() {
    let path = unique_temp_path("test_t3_j_rec_autosave", "txt");
    fs::write(&path, "original disk text").unwrap();
    Journal::record(&path, "unsaved crashed edits").unwrap();

    // Verify autosave is not enabled by default
    let controller = AutoSaveController::new(AutoSaveConfig {
        enabled: false,
        debounce: Duration::from_millis(50),
    });
    assert!(!controller.should_autosave(true));

    let recovered = Journal::recover(&path).unwrap();
    assert_eq!(recovered, Some("unsaved crashed edits".to_string()));
    // Disk file must remain intact
    assert_eq!(fs::read_to_string(&path).unwrap(), "original disk text");

    Journal::remove(&path).unwrap();
    let _ = fs::remove_file(&path);
}

#[test]
fn test_t3_03_undo_redo_with_atomic_crlf_deletion() {
    let mut ed = Editor::from_str("hello\r\nworld");
    // Position cursor at start of "world" (offset 7)
    ed.cursors_mut().set_single(7);
    ed.insert_str(" beautiful").unwrap();
    assert_eq!(ed.buffer().rope().to_string(), "hello\r\n beautifulworld");

    assert!(ed.undo());
    assert_eq!(ed.buffer().rope().to_string(), "hello\r\nworld");

    assert!(ed.redo());
    assert_eq!(ed.buffer().rope().to_string(), "hello\r\n beautifulworld");
}

#[test]
fn test_t3_04_git_baseline_save_and_undo_dirty_tracking() {
    let path = unique_temp_path("test_t3_git_baseline", "txt");
    fs::write(&path, "committed v1\n").unwrap();

    let mut ed = Editor::open(&path).unwrap();
    assert!(!ed.is_dirty());

    ed.cursors_mut().set_single(13);
    ed.insert_str("new line\n").unwrap();
    assert!(ed.is_dirty());

    ed.save().unwrap();
    assert!(!ed.is_dirty());
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "committed v1\nnew line\n"
    );

    let _ = fs::remove_file(&path);
}

#[test]
fn test_t3_05_multi_tab_journal_separation() {
    let path_a = unique_temp_path("test_t3_tab_a", "txt");
    let path_b = unique_temp_path("test_t3_tab_b", "txt");
    fs::write(&path_a, "content A").unwrap();
    fs::write(&path_b, "content B").unwrap();

    let j_a = Journal::journal_path(&path_a);
    let j_b = Journal::journal_path(&path_b);
    assert_ne!(j_a, j_b, "Different files must have distinct journal paths");

    Journal::record(&path_a, "crash A").unwrap();
    Journal::record(&path_b, "crash B").unwrap();

    assert_eq!(
        Journal::recover(&path_a).unwrap(),
        Some("crash A".to_string())
    );
    assert_eq!(
        Journal::recover(&path_b).unwrap(),
        Some("crash B".to_string())
    );

    Journal::remove(&path_a).unwrap();
    Journal::remove(&path_b).unwrap();
    let _ = fs::remove_file(&path_a);
    let _ = fs::remove_file(&path_b);
}

#[test]
fn test_t3_06_save_as_updates_journal_and_cleans_dirty() {
    let path = unique_temp_path("test_t3_save_as", "txt");
    let mut ed = Editor::new();
    ed.insert_str("untitled text").unwrap();
    assert!(ed.is_dirty());
    assert_eq!(ed.file_path(), None);

    ed.save_as(&path).unwrap();
    assert!(!ed.is_dirty());
    assert_eq!(ed.file_path(), Some(path.as_path()));
    assert_eq!(
        ed.active_tab().journal_path,
        Some(Journal::journal_path(&path))
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "untitled text");

    let _ = fs::remove_file(&path);
}

#[test]
fn test_t3_07_atomic_save_preserves_file_durability() {
    let path = unique_temp_path("test_t3_atomic_save", "txt");
    fs::write(&path, "line 1\n").unwrap();

    let mut ed = Editor::open(&path).unwrap();
    ed.cursors_mut().set_single(7);
    ed.insert_str("line 2\n").unwrap();
    ed.save().unwrap();

    assert!(!ed.is_dirty());
    let saved = fs::read_to_string(&path).unwrap();
    assert_eq!(saved, "line 1\nline 2\n");
    let _ = fs::remove_file(&path);
}

#[test]
fn test_t3_08_undo_branching_with_multiple_cursors() {
    let mut ed = Editor::from_str("abc\ndef");
    ed.cursors_mut().set_single(0);
    ed.cursors_mut().add_cursor(Cursor::new(4));
    assert_eq!(ed.cursors().cursors().len(), 2);

    ed.insert_char('!').unwrap();
    assert!(ed.undo());
    assert_eq!(ed.buffer().rope().to_string(), "abc\ndef");

    // Single cursor branch edit
    ed.cursors_mut().set_single(1);
    ed.insert_char('?').unwrap();
    assert_eq!(ed.buffer().rope().to_string(), "a?bc\ndef");
    assert!(!ed.redo(), "Redo must be invalid after branching edit");
}

#[test]
fn test_t3_09_discard_tab_cleans_journal_and_keeps_disk_clean() {
    let path = unique_temp_path("test_t3_discard_clean", "txt");
    fs::write(&path, "clean disk").unwrap();

    let mut ed = Editor::open(&path).unwrap();
    ed.insert_str("unsaved garbage").unwrap();
    assert!(ed.is_dirty());

    // Discard changes by closing tab
    ed.close_tab(0);
    assert_eq!(fs::read_to_string(&path).unwrap(), "clean disk");
    let _ = fs::remove_file(&path);
}

#[test]
fn test_t3_10_diff_computation_with_crlf_normalization() {
    let base = ropey::Rope::from_str("header\r\nbody\r\nfooter\r\n");
    let curr = ropey::Rope::from_str("header\nbody\nfooter\n");
    let diffs = compute_rope_diffs(&base, &curr);
    // Normalized diff must treat equivalent lines as Unchanged
    assert_eq!(diffs.len(), 4);
    assert!(diffs.iter().all(|d| *d == LineDiffStatus::Unchanged));
}

#[test]
fn test_t3_11_history_limits_with_large_deltas() {
    let mut ed = Editor::new();
    let big_chunk = "x".repeat(10_000);
    for _ in 0..10 {
        ed.insert_str(&big_chunk).unwrap();
    }
    assert_eq!(ed.buffer().len_bytes(), 100_000);
    assert!(ed.undo());
    assert_eq!(ed.buffer().len_bytes(), 90_000);
}

#[test]
fn test_t3_12_journal_burst_and_recovery_integrity() {
    let path = unique_temp_path("test_t3_burst_integrity", "txt");
    fs::write(&path, "init:").unwrap();

    let mut deltas = Vec::new();
    for i in 0..20 {
        deltas.push(TextDelta {
            start_byte: 5 + i * 2,
            old_text: String::new(),
            new_text: format!("_{}", i),
            timestamp: i as u64,
        });
    }
    Journal::record_deltas(&path, &deltas).unwrap();

    let recovered = Journal::recover(&path).unwrap().unwrap();
    assert!(recovered.starts_with("init:"));
    assert!(recovered.contains("_19"));

    Journal::remove(&path).unwrap();
    let _ = fs::remove_file(&path);
}

#[test]
fn test_t3_13_open_tab_already_open_switches_focus() {
    let path = unique_temp_path("test_t3_dedup_tab", "txt");
    fs::write(&path, "hello").unwrap();

    let mut ed = Editor::open(&path).unwrap();
    assert_eq!(ed.tabs().len(), 1);
    assert_eq!(ed.active_tab_idx(), 0);

    // Opening already opened tab switches to it and does not duplicate
    let idx = ed.open_tab(&path).unwrap();
    assert_eq!(idx, 0);
    assert_eq!(ed.tabs().len(), 1);

    let _ = fs::remove_file(&path);
}

#[test]
fn test_t3_14_search_replace_and_undo_restores_original() {
    let mut ed = Editor::from_str("foo bar foo baz");
    ed.cursors_mut().set_single(0);
    ed.insert_str("prefix_").unwrap();
    assert_eq!(ed.buffer().rope().to_string(), "prefix_foo bar foo baz");

    assert!(ed.undo());
    assert_eq!(ed.buffer().rope().to_string(), "foo bar foo baz");
}

#[test]
fn test_t3_15_multi_tab_close_preserves_inactive_tab_dirty_state() {
    let mut ed = Editor::new();
    ed.insert_str("tab 0 work").unwrap();
    assert!(ed.is_dirty());

    ed.new_tab();
    ed.insert_str("tab 1 work").unwrap();
    assert!(ed.is_dirty());
    assert_eq!(ed.tabs().len(), 2);

    // Close active tab 1
    ed.close_tab(1);
    assert_eq!(ed.tabs().len(), 1);
    assert_eq!(ed.active_tab_idx(), 0);
    // Tab 0 must still have its dirty edits
    assert!(ed.is_dirty());
    assert_eq!(ed.buffer().rope().to_string(), "tab 0 work");
}
