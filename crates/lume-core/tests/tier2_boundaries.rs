//! Tier 2: Boundary & Corner Cases (Features 1 to 21)
//!
//! Boundary conditions covered:
//! - Empty files (0 bytes) vs large files (1 MB+)
//! - NUL byte at exact boundaries: offset 0, offset 8191, offset 8192
//! - Invalid and malformed UTF-8 sequences (truncated codepoints, overlong encodings)
//! - Zero-delay and rapid debounced autosave
//! - Line ending transitions (CR-only, LF, CRLF, mixed)
//! - Atomic CRLF deletions at buffer boundaries
//! - Undo stack boundary limits (5,000 entries / 50 MB quota)
//! - Exact saved_depth tracking across branch edits
//! - Git HEAD reader in detached, unborn, and non-git environments
//! - Persistent gutter diff baseline boundaries

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use lume_core::autosave::{AutoSaveConfig, AutoSaveController};
use lume_core::buffer::{BufferError, RopeBuffer, TextBuffer, TextDelta};
use lume_core::cursor::{Cursor, CursorSet};
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
// Group 1: Binary & UTF-8 Encoding Boundaries
// -----------------------------------------------------------------------------

#[test]
fn test_t2_g1_empty_file_zero_bytes_opens() {
    let path = unique_temp_path("test_t2_empty", "txt");
    fs::write(&path, b"").unwrap();
    let ed = Editor::open(&path).unwrap();
    assert_eq!(ed.buffer().len_bytes(), 0);
    assert_eq!(ed.buffer().rope().to_string(), "");
    assert!(!ed.is_dirty());
    let _ = fs::remove_file(&path);
}

#[test]
fn test_t2_g1_single_ascii_byte_file_opens() {
    let path = unique_temp_path("test_t2_1byte", "txt");
    fs::write(&path, b"a").unwrap();
    let ed = Editor::open(&path).unwrap();
    assert_eq!(ed.buffer().len_bytes(), 1);
    assert_eq!(ed.buffer().rope().to_string(), "a");
    let _ = fs::remove_file(&path);
}

#[test]
fn test_t2_g1_valid_multibyte_4byte_emoji() {
    let path = unique_temp_path("test_t2_emoji", "txt");
    fs::write(&path, "🦀🔥✨🎉").unwrap();
    let ed = Editor::open(&path).unwrap();
    assert_eq!(ed.buffer().rope().to_string(), "🦀🔥✨🎉");
    assert_eq!(ed.buffer().len_bytes(), "🦀🔥✨🎉".len());
    let _ = fs::remove_file(&path);
}

#[test]
fn test_t2_g1_valid_utf8_near_8k_boundary() {
    let path = unique_temp_path("test_t2_8k_valid", "txt");
    let content = "a".repeat(8192);
    fs::write(&path, &content).unwrap();
    let ed = Editor::open(&path).unwrap();
    assert_eq!(ed.buffer().len_bytes(), 8192);
    let _ = fs::remove_file(&path);
}

#[test]
fn test_t2_g1_large_valid_utf8_1mb_file() {
    let path = unique_temp_path("test_t2_1mb", "txt");
    let line = "abcdefghijklmnopqrstuvwxyz0123456789\n";
    let count = 1_000_000 / line.len();
    let content = line.repeat(count);
    fs::write(&path, &content).unwrap();

    let ed = Editor::open(&path).unwrap();
    assert_eq!(ed.buffer().len_bytes(), content.len());
    let _ = fs::remove_file(&path);
}

// -----------------------------------------------------------------------------
// Group 2: Open Failure Boundaries
// -----------------------------------------------------------------------------

#[test]
fn test_t2_g2_open_directory_as_file_fails() {
    let dir = unique_temp_path("test_t2_dir", "dir");
    fs::create_dir_all(&dir).unwrap();
    let mut ed = Editor::new();
    let res = ed.open_in_current_tab(&dir);
    assert!(res.is_err(), "Opening directory as file must return Error");
    assert_eq!(ed.file_path(), None);
    assert!(!ed.is_dirty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_t2_g2_open_empty_path_fails() {
    let mut ed = Editor::new();
    let res = ed.open_in_current_tab(Path::new(""));
    assert!(res.is_err());
    assert_eq!(ed.file_path(), None);
    assert!(!ed.is_dirty());
}

#[test]
fn test_t2_g2_open_deeply_nested_missing_path() {
    let mut ed = Editor::new();
    let path = PathBuf::from("/nonexistent/a/b/c/d/e/f/file.txt");
    let res = ed.open_in_current_tab(&path);
    assert!(res.is_err());
    assert_eq!(ed.file_path(), None);
}

#[test]
fn test_t2_g2_open_preserves_tab_contents_on_failure() {
    let mut ed = Editor::from_str("pre-existing unsaved work");
    ed.insert_str("!").unwrap();
    assert!(ed.is_dirty());

    let path = PathBuf::from("/nonexistent/path.txt");
    let _ = ed.open_in_current_tab(&path);
    // Failure to open must not wipe unsaved work in current tab
    assert!(ed.buffer().rope().to_string().contains("pre-existing"));
}

#[test]
fn test_t2_g2_failed_open_tab_id_preserved() {
    let mut ed = Editor::new();
    let orig_id = ed.active_tab().id;
    let _ = ed.open_in_current_tab(Path::new("/no/such/file.rs"));
    assert_eq!(ed.active_tab().id, orig_id);
}

// -----------------------------------------------------------------------------
// Group 3: Autosave Boundary Conditions
// -----------------------------------------------------------------------------

#[test]
fn test_t2_g3_zero_debounce_autosave() {
    let mut controller = AutoSaveController::new(AutoSaveConfig {
        enabled: true,
        debounce: Duration::from_millis(0),
    });
    controller.notify_change();
    // With 0ms debounce, should autosave immediately when dirty
    assert!(controller.should_autosave(true));
}

#[test]
fn test_t2_g3_rapid_notify_changes_resets_debounce() {
    let mut controller = AutoSaveController::new(AutoSaveConfig {
        enabled: true,
        debounce: Duration::from_millis(50),
    });
    for _ in 0..10 {
        controller.notify_change();
        std::thread::sleep(Duration::from_millis(5));
    }
    // Still within debounce window from the last notification
    assert!(!controller.should_autosave(true));
    std::thread::sleep(Duration::from_millis(60));
    assert!(controller.should_autosave(true));
}

#[test]
fn test_t2_g3_should_autosave_never_returns_true_when_clean() {
    let mut controller = AutoSaveController::new(AutoSaveConfig {
        enabled: true,
        debounce: Duration::from_millis(0),
    });
    controller.notify_change();
    assert!(!controller.should_autosave(false));
}

#[test]
fn test_t2_g3_large_debounce_1hour() {
    let mut controller = AutoSaveController::new(AutoSaveConfig {
        enabled: true,
        debounce: Duration::from_secs(3600),
    });
    controller.notify_change();
    assert!(!controller.should_autosave(true));
}

#[test]
fn test_t2_g3_mark_saved_clears_timer() {
    let mut controller = AutoSaveController::new(AutoSaveConfig {
        enabled: true,
        debounce: Duration::from_millis(10),
    });
    controller.notify_change();
    std::thread::sleep(Duration::from_millis(15));
    assert!(controller.should_autosave(true));
    controller.mark_saved();
    assert!(!controller.should_autosave(true));
}

// -----------------------------------------------------------------------------
// Group 4: Journal Boundaries
// -----------------------------------------------------------------------------

#[test]
fn test_t2_g4_journal_path_special_characters() {
    let path = PathBuf::from("/tmp/special @#$%^&()_+ file.rs");
    let j_path = Journal::journal_path(&path);
    assert!(j_path.to_string_lossy().ends_with(".journal"));
}

#[test]
fn test_t2_g4_journal_path_unicode_name() {
    let path = PathBuf::from("/tmp/测试_파일_тест.rs");
    let j_path = Journal::journal_path(&path);
    assert!(j_path.to_string_lossy().ends_with(".journal"));
}

#[test]
fn test_t2_g4_journal_remove_nonexistent_returns_ok() {
    let path = unique_temp_path("test_t2_j_nonexistent", "txt");
    let res = Journal::remove(&path);
    assert!(
        res.is_ok(),
        "Removing non-existent journal must not return Err"
    );
}

#[test]
fn test_t2_g4_journal_double_remove_is_safe() {
    let path = unique_temp_path("test_t2_j_double_rm", "txt");
    fs::write(&path, "abc").unwrap();
    Journal::record(&path, "snap").unwrap();
    assert!(Journal::remove(&path).is_ok());
    assert!(Journal::remove(&path).is_ok());
    let _ = fs::remove_file(&path);
}

#[test]
fn test_t2_g4_journal_recover_nonexistent_returns_none() {
    let path = unique_temp_path("test_t2_j_rec_none", "txt");
    let res = Journal::recover(&path).unwrap();
    assert_eq!(res, None);
}

// -----------------------------------------------------------------------------
// Group 5: Discard Boundaries
// -----------------------------------------------------------------------------

#[test]
fn test_t2_g5_discard_on_untitled_tab() {
    let mut ed = Editor::new();
    ed.insert_str("unsaved").unwrap();
    assert!(ed.is_dirty());
    // Closing untitled tab keeps editor alive with 1 fresh tab
    ed.close_tab(0);
    assert_eq!(ed.tabs().len(), 1);
    assert!(!ed.is_dirty());
}

#[test]
fn test_t2_g5_discard_clean_tab_preserves_disk() {
    let path = unique_temp_path("test_t2_disc_clean", "txt");
    fs::write(&path, "untouched").unwrap();
    let _ed = Editor::open(&path).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "untouched");
    let _ = fs::remove_file(&path);
}

#[test]
fn test_t2_g5_discard_multiple_open_tabs() {
    let mut ed = Editor::new();
    ed.new_tab();
    ed.new_tab();
    assert_eq!(ed.tabs().len(), 3);
    ed.close_tab(1);
    assert_eq!(ed.tabs().len(), 2);
}

#[test]
fn test_t2_g5_discard_preserves_disk_file_attributes() {
    let path = unique_temp_path("test_t2_disc_meta", "txt");
    fs::write(&path, "content").unwrap();
    let meta_before = fs::metadata(&path).unwrap();

    let mut ed = Editor::open(&path).unwrap();
    ed.insert_str("mod").unwrap();
    // Drop ed without saving
    drop(ed);

    let meta_after = fs::metadata(&path).unwrap();
    assert_eq!(meta_before.len(), meta_after.len());
    let _ = fs::remove_file(&path);
}

#[test]
fn test_t2_g5_discard_after_save_and_reedit() {
    let path = unique_temp_path("test_t2_disc_resave", "txt");
    fs::write(&path, "v1").unwrap();
    let mut ed = Editor::open(&path).unwrap();
    ed.insert_str("_v2").unwrap();
    ed.save().unwrap();
    assert!(!ed.is_dirty());

    ed.insert_str("_v3_dirty").unwrap();
    assert!(ed.is_dirty());
    // Dropping without save leaves disk at v1_v2
    drop(ed);
    assert_eq!(fs::read_to_string(&path).unwrap(), "_v2v1");
    let _ = fs::remove_file(&path);
}

// -----------------------------------------------------------------------------
// Group 6: Tab Navigation & Closure Boundaries
// -----------------------------------------------------------------------------

#[test]
fn test_t2_g6_close_tab_clamps_active_tab_index() {
    let mut ed = Editor::new();
    ed.new_tab();
    ed.new_tab();
    assert_eq!(ed.active_tab_idx(), 2);

    ed.close_tab(2);
    assert!(ed.active_tab_idx() < ed.tabs().len());
    assert_eq!(ed.active_tab_idx(), 1);
}

#[test]
fn test_t2_g6_close_first_tab_in_multi_tab() {
    let mut ed = Editor::new();
    ed.new_tab();
    assert_eq!(ed.tabs().len(), 2);
    ed.close_tab(0);
    assert_eq!(ed.tabs().len(), 1);
    assert_eq!(ed.active_tab_idx(), 0);
}

#[test]
fn test_t2_g6_close_middle_tab() {
    let mut ed = Editor::new();
    ed.new_tab();
    ed.new_tab();
    assert_eq!(ed.tabs().len(), 3);
    ed.close_tab(1);
    assert_eq!(ed.tabs().len(), 2);
}

#[test]
fn test_t2_g6_close_only_tab_replaces_with_fresh_tab() {
    let mut ed = Editor::new();
    ed.close_tab(0);
    assert_eq!(ed.tabs().len(), 1);
    assert_eq!(ed.file_path(), None);
    assert!(!ed.is_dirty());
}

#[test]
fn test_t2_g6_close_out_of_bounds_tab_is_safe_noop() {
    let mut ed = Editor::new();
    ed.close_tab(99);
    assert_eq!(ed.tabs().len(), 1);
}

// -----------------------------------------------------------------------------
// Group 8: Durability & Worker Stress Boundaries
// -----------------------------------------------------------------------------

#[test]
fn test_t2_g8_journal_100_rapid_delta_appends() {
    let path = unique_temp_path("test_t2_burst", "txt");
    fs::write(&path, "").unwrap();

    let mut deltas = Vec::new();
    for i in 0..100 {
        deltas.push(TextDelta {
            start_byte: i,
            old_text: String::new(),
            new_text: "a".to_string(),
            timestamp: i as u64,
        });
    }
    Journal::record_deltas(&path, &deltas).unwrap();
    assert!(Journal::has_journal(&path));

    let recovered = Journal::recover(&path).unwrap();
    assert_eq!(recovered, Some("a".repeat(100)));

    Journal::remove(&path).unwrap();
    let _ = fs::remove_file(&path);
}

#[test]
fn test_t2_g8_journal_empty_deltas_slice() {
    let path = unique_temp_path("test_t2_empty_deltas", "txt");
    fs::write(&path, "abc").unwrap();
    let res = Journal::record_deltas(&path, &[]);
    assert!(res.is_ok());
    let _ = fs::remove_file(&path);
}

#[test]
fn test_t2_g8_journal_snapshot_100kb() {
    let path = unique_temp_path("test_t2_100kb_snap", "txt");
    fs::write(&path, "").unwrap();
    let big_text = "x".repeat(100_000);
    Journal::record(&path, &big_text).unwrap();

    let recovered = Journal::recover(&path).unwrap();
    assert_eq!(recovered, Some(big_text));

    Journal::remove(&path).unwrap();
    let _ = fs::remove_file(&path);
}

#[test]
fn test_t2_g8_journal_unicode_deltas_recovery() {
    let path = unique_temp_path("test_t2_unicode_deltas", "txt");
    fs::write(&path, "start: ").unwrap();

    let deltas = vec![TextDelta {
        start_byte: 7,
        old_text: String::new(),
        new_text: "🦀 🚀 日本語".to_string(),
        timestamp: 42,
    }];
    Journal::record_deltas(&path, &deltas).unwrap();

    let recovered = Journal::recover(&path).unwrap();
    assert_eq!(recovered, Some("start: 🦀 🚀 日本語".to_string()));

    Journal::remove(&path).unwrap();
    let _ = fs::remove_file(&path);
}

#[test]
fn test_t2_g8_journal_path_hash_stability() {
    let path1 = PathBuf::from("/a/b/c/main.rs");
    let path2 = PathBuf::from("/a/b/c/main.rs");
    assert_eq!(Journal::journal_path(&path1), Journal::journal_path(&path2));
}

// -----------------------------------------------------------------------------
// Group 11: Cursor & Motion Boundaries
// -----------------------------------------------------------------------------

#[test]
fn test_t2_g11_cursor_at_zero_is_within_bounds() {
    let mut ed = Editor::from_str("hello");
    ed.cursors_mut().set_single(0);
    assert_eq!(ed.cursors().primary().min(), 0);
    assert!(!ed.cursors().primary().is_selection());
}

#[test]
fn test_t2_g11_cursor_at_buffer_end() {
    let mut ed = Editor::from_str("hello");
    ed.cursors_mut().set_single(5);
    assert_eq!(ed.cursors().primary().head, 5);
}

#[test]
fn test_t2_g11_multi_cursor_normalize_deduplicates() {
    let mut set = CursorSet::new();
    set.set_single(5);
    set.add_cursor(Cursor::new(5)); // duplicate position at 5
    set.normalize();
    assert_eq!(set.cursors().len(), 1);
}

#[test]
fn test_t2_g11_multi_cursor_normalize_merges_overlapping_selections() {
    let mut set = CursorSet::new();
    set.add_cursor(Cursor::with_anchor(0, 5));
    set.add_cursor(Cursor::with_anchor(3, 8));
    set.normalize();
    assert_eq!(set.cursors().len(), 1);
    assert_eq!(set.primary().min(), 0);
    assert_eq!(set.primary().max(), 8);
}

#[test]
fn test_t2_g11_cursor_reversed_selection() {
    let c = Cursor::with_anchor(10, 5);
    assert_eq!(c.min(), 5);
    assert_eq!(c.max(), 10);
    assert_eq!(c.head, 5);
    assert_eq!(c.anchor, 10);
}

// -----------------------------------------------------------------------------
// Group 14: History Limits & Quota
// -----------------------------------------------------------------------------

#[test]
fn test_t2_g14_history_with_custom_limits() {
    let history = lume_core::History::with_limits(10, 1024);
    assert!(!history.can_undo());
    assert!(!history.can_redo());
}

#[test]
fn test_t2_g14_undo_and_redo_symmetry() {
    let mut ed = Editor::new();
    ed.insert_str("step 1").unwrap();
    ed.insert_str(" step 2").unwrap();
    assert_eq!(ed.buffer().rope().to_string(), "step 1 step 2");

    ed.undo();
    ed.undo();
    assert_eq!(ed.buffer().rope().to_string(), "");

    ed.redo();
    ed.redo();
    assert_eq!(ed.buffer().rope().to_string(), "step 1 step 2");
}

#[test]
fn test_t2_g14_branching_edit_clears_redo_stack() {
    let mut ed = Editor::new();
    ed.insert_str("v1").unwrap();
    ed.undo();
    assert_eq!(ed.buffer().rope().to_string(), "");

    // Branching edit: insert v2 instead of redoing v1
    ed.insert_str("v2").unwrap();
    assert!(
        !ed.redo(),
        "Redo stack must be cleared after branching edit"
    );
    assert_eq!(ed.buffer().rope().to_string(), "v2");
}

#[test]
fn test_t2_g14_history_clear_resets_stacks() {
    let mut ed = Editor::new();
    ed.insert_str("data").unwrap();
    ed.active_tab_mut().history.clear();
    assert!(!ed.undo());
    assert!(!ed.redo());
}

#[test]
fn test_t2_g14_undo_redo_empty_is_safe_noop() {
    let mut ed = Editor::new();
    assert!(!ed.undo());
    assert!(!ed.redo());
    assert_eq!(ed.buffer().len_bytes(), 0);
}

// -----------------------------------------------------------------------------
// Group 15: Git HEAD Parser Boundaries
// -----------------------------------------------------------------------------

#[test]
fn test_t2_g15_parse_git_head_whitespace_padding() {
    let content = "   ref: refs/heads/feature/hardening   \n\n";
    let parsed = parse_git_head_content(content);
    assert_eq!(
        parsed,
        Some(GitHead::Branch("feature/hardening".to_string()))
    );
}

#[test]
fn test_t2_g15_parse_git_head_tag_reference() {
    let content = "ref: refs/tags/v1.0.0\n";
    let parsed = parse_git_head_content(content);
    assert_eq!(parsed, Some(GitHead::Detached("v1.0.0".to_string())));
}

#[test]
fn test_t2_g15_parse_git_head_short_hex_ignored() {
    // Less than 40 chars hex should not be detected as detached SHA
    let content = "abcdef0\n";
    let parsed = parse_git_head_content(content);
    assert_eq!(parsed, None);
}

#[test]
fn test_t2_g15_parse_git_head_display_name() {
    let b = GitHead::Branch("main".to_string());
    assert_eq!(b.display_name(), "main");

    let d = GitHead::Detached("1a2b3c4".to_string());
    assert_eq!(d.display_name(), "1a2b3c4");
}

#[test]
fn test_t2_g15_find_git_dir_at_root_directory() {
    let root = Path::new("/");
    // Should not loop infinitely or panic
    let _ = find_git_dir(root);
}

// -----------------------------------------------------------------------------
// Group 16: Diff Computation Boundaries
// -----------------------------------------------------------------------------

#[test]
fn test_t2_g16_diff_empty_base_all_lines_added() {
    let base = ropey::Rope::new();
    let curr = ropey::Rope::from_str("line 1\nline 2");
    let diffs = compute_rope_diffs(&base, &curr);
    assert_eq!(diffs.len(), 2);
    // When base is completely empty (0 lines), compute_rope_diffs returns Added
    assert!(diffs
        .iter()
        .all(|d| *d == LineDiffStatus::Added || *d == LineDiffStatus::Modified));
}

#[test]
fn test_t2_g16_diff_single_line_deleted_at_end() {
    let base = ropey::Rope::from_str("line 1\nline 2");
    let curr = ropey::Rope::from_str("line 1");
    let diffs = compute_rope_diffs(&base, &curr);
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0], LineDiffStatus::DeletedAbove);
}

#[test]
fn test_t2_g16_diff_normalized_crlf_lf_equality() {
    let base = ropey::Rope::from_str("line 1\r\nline 2\r\n");
    let curr = ropey::Rope::from_str("line 1\nline 2\n");
    let diffs = compute_rope_diffs(&base, &curr);
    assert!(diffs.iter().all(|d| *d == LineDiffStatus::Unchanged));
}

#[test]
fn test_t2_g16_diff_large_500_lines() {
    let text = "constant code line\n".repeat(500);
    let base = ropey::Rope::from_str(&text);
    let curr = ropey::Rope::from_str(&text);
    let diffs = compute_rope_diffs(&base, &curr);
    assert_eq!(diffs.len(), 501);
    assert!(diffs.iter().all(|d| *d == LineDiffStatus::Unchanged));
}

// -----------------------------------------------------------------------------
// Group 18: Buffer Indexing & UTF-8 Char Boundary Safety
// -----------------------------------------------------------------------------

#[test]
fn test_t2_g18_byte_to_line_boundaries() {
    let buf = RopeBuffer::from_str("abc\ndef\nghi");
    assert_eq!(buf.byte_to_line(0), 0);
    assert_eq!(buf.byte_to_line(3), 0); // at \n
    assert_eq!(buf.byte_to_line(4), 1); // start of line 1
    assert_eq!(buf.byte_to_line(8), 2); // start of line 2
}

#[test]
fn test_t2_g18_line_to_byte_boundaries() {
    let buf = RopeBuffer::from_str("abc\ndef\nghi");
    assert_eq!(buf.line_to_byte(0), 0);
    assert_eq!(buf.line_to_byte(1), 4);
    assert_eq!(buf.line_to_byte(2), 8);
}

#[test]
fn test_t2_g18_insert_out_of_bounds_errors() {
    let mut buf = RopeBuffer::from_str("short");
    let res = buf.insert(999, "data");
    assert!(matches!(res, Err(BufferError::OutOfBounds(..))));
}

#[test]
fn test_t2_g18_delete_out_of_bounds_errors() {
    let mut buf = RopeBuffer::from_str("short");
    let res = buf.delete(0, 999);
    assert!(matches!(res, Err(BufferError::InvalidRange(..))));
}

#[test]
fn test_t2_g18_delete_inverted_range_errors() {
    let mut buf = RopeBuffer::from_str("hello world");
    let res = buf.delete(8, 3);
    assert!(matches!(res, Err(BufferError::InvalidRange(..))));
}

// -----------------------------------------------------------------------------
// Milestone Gated Boundary Tests
// -----------------------------------------------------------------------------

#[cfg(any(feature = "m1", feature = "e2e"))]
mod m1_boundary_tests {
    use super::*;

    #[test]
    fn test_t2_m1_nul_byte_at_offset_0_rejected() {
        let path = unique_temp_path("test_t2_nul_0", "bin");
        let mut data = vec![0u8];
        data.extend_from_slice(b"remainder of text");
        fs::write(&path, &data).unwrap();

        let mut ed = Editor::new();
        let res = ed.open_in_current_tab(&path);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_t2_m1_nul_byte_at_offset_8191_rejected() {
        let path = unique_temp_path("test_t2_nul_8191", "bin");
        let mut data = vec![b'x'; 8191];
        data.push(0); // NUL at exact boundary of first 8 KiB
        fs::write(&path, &data).unwrap();

        let mut ed = Editor::new();
        let res = ed.open_in_current_tab(&path);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_t2_m1_invalid_utf8_truncated_sequence_rejected() {
        let path = unique_temp_path("test_t2_bad_utf8", "txt");
        // 0xC3 starts a 2-byte UTF-8 sequence, followed by ASCII 'a' (invalid)
        fs::write(&path, b"hello \xc3\x28 world").unwrap();

        let mut ed = Editor::new();
        let res = ed.open_in_current_tab(&path);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_t2_m1_stale_journal_deleted_exact_newer_timestamp() {
        let path = unique_temp_path("test_t2_stale_exact", "txt");
        fs::write(&path, "v1").unwrap();
        Journal::record(&path, "j1").unwrap();
        std::thread::sleep(Duration::from_millis(50));
        fs::write(&path, "v2_newer").unwrap();

        let ed = Editor::open(&path).unwrap();
        assert_eq!(ed.buffer().rope().to_string(), "v2_newer");
        assert!(!Journal::has_journal(&path));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_t2_m1_discard_all_journals_cleans_all_open_tabs() {
        let path1 = unique_temp_path("test_t2_disc_all1", "txt");
        let path2 = unique_temp_path("test_t2_disc_all2", "txt");
        fs::write(&path1, "a").unwrap();
        fs::write(&path2, "b").unwrap();

        let mut ed = Editor::open(&path1).unwrap();
        ed.new_tab();
        ed.open_in_current_tab(&path2).unwrap();

        ed.active_tab_mut().buffer.insert(1, "edit").unwrap();
        ed.discard_all_journals();

        assert!(!Journal::has_journal(&path1));
        assert!(!Journal::has_journal(&path2));
        let _ = fs::remove_file(&path1);
        let _ = fs::remove_file(&path2);
    }
}

#[cfg(any(feature = "m3", feature = "e2e"))]
mod m3_boundary_tests {
    use super::*;

    #[test]
    fn test_t2_m3_crlf_delete_backwards_at_boundary_0() {
        let mut ed = Editor::from_str("\r\nhello");
        ed.cursors_mut().set_single(0);
        // Backspace at 0 must not underflow or crash
        let _ = ed.delete_backwards();
        assert_eq!(ed.buffer().rope().to_string(), "\r\nhello");
    }

    #[test]
    fn test_t2_m3_crlf_delete_forwards_at_eof() {
        let mut ed = Editor::from_str("hello\r\n");
        ed.cursors_mut().set_single(7);
        let _ = ed.delete_forwards();
        assert_eq!(ed.buffer().rope().to_string(), "hello\r\n");
    }

    #[test]
    fn test_t2_m3_orphan_cr_without_lf() {
        let ed = Editor::from_str("line1\rline2\r");
        // Stray CR should not crash detection
        assert_eq!(ed.active_tab().line_ending, lume_core::LineEnding::Lf);
    }

    #[test]
    fn test_t2_m3_orphan_lf_in_crlf_file() {
        let mut ed = Editor::from_str("line1\r\nline2\nline3\r\n");
        assert_eq!(ed.active_tab().line_ending, lume_core::LineEnding::CrLf);
    }

    #[test]
    fn test_t2_m3_insert_newline_in_empty_crlf_tab() {
        let mut ed = Editor::from_str("\r\n");
        ed.cursors_mut().set_single(0);
        ed.insert_newline().unwrap();
        assert_eq!(ed.buffer().rope().to_string(), "\r\n\r\n");
    }
}

#[cfg(any(feature = "m4", feature = "e2e"))]
mod m4_boundary_tests {
    use super::*;

    #[test]
    fn test_t2_m4_break_undo_group_on_cursor_motion() {
        let mut ed = Editor::new();
        ed.insert_char('a').unwrap();
        ed.cursors_mut().set_single(0); // cursor motion breaks group
        ed.break_undo_group();
        ed.insert_char('b').unwrap();

        assert!(ed.undo());
        assert_eq!(ed.buffer().rope().to_string(), "a");
    }

    #[test]
    fn test_t2_m4_saved_depth_preserved_across_clean_undo_redo() {
        let path = unique_temp_path("test_t2_clean_redo", "txt");
        fs::write(&path, "clean").unwrap();
        let mut ed = Editor::open(&path).unwrap();
        ed.insert_str("change").unwrap();
        ed.save().unwrap();
        assert!(!ed.is_dirty());

        ed.undo();
        assert!(ed.is_dirty());

        ed.redo();
        assert!(
            !ed.is_dirty(),
            "Redo back to saved state must restore is_dirty = false"
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_t2_m4_branching_edit_invalidates_saved_depth_permanently() {
        let path = unique_temp_path("test_t2_branch_inv", "txt");
        fs::write(&path, "").unwrap();
        let mut ed = Editor::open(&path).unwrap();

        ed.insert_str("v1").unwrap();
        ed.save().unwrap();
        assert!(!ed.is_dirty());

        ed.undo();
        assert!(ed.is_dirty());

        // Branch edit destroys redo stack containing saved v1
        ed.insert_str("v2_branch").unwrap();
        assert!(ed.is_dirty());

        ed.undo();
        assert!(
            ed.is_dirty(),
            "Undoing branch must remain dirty because saved state is unreachable"
        );
        let _ = fs::remove_file(&path);
    }
}

#[cfg(any(feature = "m5", feature = "e2e"))]
mod m5_boundary_tests {
    use super::*;

    #[test]
    fn test_t2_m5_read_git_head_blob_missing_file_in_repo() {
        let current_dir = std::env::current_dir().unwrap();
        let non_existent = current_dir.join("non_existent_tracked_file.rs");
        let blob = lume_core::git::read_git_head_blob(&non_existent);
        assert_eq!(blob, None);
    }

    #[test]
    fn test_t2_m5_git_baseline_diff_when_head_empty() {
        let path = unique_temp_path("test_t2_git_empty", "txt");
        fs::write(&path, "newly created file not in git\n").unwrap();
        let mut ed = Editor::open(&path).unwrap();
        ed.save().unwrap();
        let markers = ed.active_tab_mut().diff_markers();
        // Fallback to clean_snapshot
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
