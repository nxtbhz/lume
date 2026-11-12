//! Adversarial Challenger 2 Tests: Milestone 1 Crash Recovery & Cleanup
//!
//! Stress-tests:
//! 1. Journal mtime comparisons (stale deletion vs fresh recovery staging).
//! 2. Physical `.journal` file deletion on `discard_tab_changes` and `discard_all_journals`.
//! 3. Proptest generators for recovery decision matrix and timestamp boundaries.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use lume_core::buffer::TextDelta;
use lume_core::editor::Editor;
use lume_core::journal::Journal;
use proptest::prelude::*;

fn unique_temp_path(prefix: &str, ext: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "{}_{}_{}_{}.{}",
        prefix,
        std::process::id(),
        count,
        nanos,
        ext
    ))
}

#[test]
fn test_adv_stale_journal_mtime_disk_strictly_newer_deletes_journal() {
    let path = unique_temp_path("test_adv_stale", "txt");
    fs::write(&path, "v1_initial_disk").unwrap();

    // Create journal with older edits
    Journal::record(&path, "v1_crashed_unsaved_edits").unwrap();
    let jpath = Journal::journal_path(&path);
    assert!(jpath.exists(), "Journal must physically exist on disk");

    // Sleep to ensure distinct filesystem mtime
    std::thread::sleep(Duration::from_millis(40));

    // Overwrite disk file with newer content
    fs::write(&path, "v2_disk_edited_externally").unwrap();

    let disk_mtime = fs::metadata(&path).unwrap().modified().unwrap();
    let journal_mtime = fs::metadata(&jpath).unwrap().modified().unwrap();
    assert!(
        disk_mtime > journal_mtime,
        "Disk file mtime must be strictly newer than journal"
    );

    // Open file in Editor
    let mut ed = Editor::new();
    ed.open_in_current_tab(&path).unwrap();

    // Assert stale journal is physically deleted from disk
    assert!(
        !jpath.exists(),
        "Stale journal must be physically removed from disk"
    );
    assert!(
        !Journal::has_journal(&path),
        "Journal::has_journal must report false"
    );

    // Assert disk content is loaded cleanly
    assert_eq!(
        ed.buffer().rope().to_string(),
        "v2_disk_edited_externally",
        "Clean disk content must be loaded"
    );
    assert!(!ed.is_dirty(), "Tab must not be dirty");
    assert!(
        ed.pending_recovery().is_none(),
        "Pending recovery must be None for stale journal"
    );

    let _ = fs::remove_file(&path);
}

#[test]
fn test_adv_fresh_journal_stages_recovery_and_loads_disk_clean() {
    let path = unique_temp_path("test_adv_fresh", "txt");
    fs::write(&path, "v1_original_disk").unwrap();

    // Sleep to ensure journal has strictly later mtime
    std::thread::sleep(Duration::from_millis(40));

    // Write crash recovery journal
    Journal::record(&path, "v1_unsaved_crash_recovery").unwrap();
    let jpath = Journal::journal_path(&path);
    assert!(jpath.exists(), "Journal must physically exist on disk");

    let disk_mtime = fs::metadata(&path).unwrap().modified().unwrap();
    let journal_mtime = fs::metadata(&jpath).unwrap().modified().unwrap();
    assert!(
        journal_mtime >= disk_mtime,
        "Journal mtime must be >= disk mtime"
    );

    // Open file in Editor
    let mut ed = Editor::new();
    ed.open_in_current_tab(&path).unwrap();

    // Assert journal still exists on disk
    assert!(
        jpath.exists(),
        "Fresh journal must be preserved on disk before user decision"
    );

    // Assert disk content is loaded cleanly (is_dirty = false)
    assert_eq!(
        ed.buffer().rope().to_string(),
        "v1_original_disk",
        "Disk content must be loaded into buffer"
    );
    assert!(
        !ed.is_dirty(),
        "Tab must NOT be marked dirty before explicit recovery"
    );
    assert_eq!(
        ed.pending_recovery(),
        Some("v1_unsaved_crash_recovery"),
        "Pending recovery must hold the recovered text"
    );

    // Now verify explicit recovery application
    assert!(
        ed.recover_tab_journal(0),
        "recover_tab_journal must return true"
    );
    assert_eq!(
        ed.buffer().rope().to_string(),
        "v1_unsaved_crash_recovery",
        "Buffer must contain recovered text"
    );
    assert!(
        ed.is_dirty(),
        "Tab must be marked dirty after explicit recovery"
    );
    assert!(
        ed.pending_recovery().is_none(),
        "Pending recovery must be cleared after recovery"
    );

    let _ = Journal::remove(&path);
    let _ = fs::remove_file(&path);
}

#[test]
fn test_adv_fresh_journal_discard_removes_physical_file() {
    let path = unique_temp_path("test_adv_fresh_discard", "txt");
    fs::write(&path, "disk_base").unwrap();

    std::thread::sleep(Duration::from_millis(40));
    Journal::record(&path, "journal_to_discard").unwrap();
    let jpath = Journal::journal_path(&path);
    assert!(jpath.exists());

    let mut ed = Editor::new();
    ed.open_in_current_tab(&path).unwrap();
    assert_eq!(ed.pending_recovery(), Some("journal_to_discard"));

    // User chooses to Discard
    assert!(
        ed.discard_tab_changes(0),
        "discard_tab_changes must return true"
    );

    // Assert physical file deleted
    assert!(
        !jpath.exists(),
        "Journal must be physically removed on discard"
    );
    assert!(!Journal::has_journal(&path));

    // Assert tab remains clean disk content
    assert_eq!(ed.buffer().rope().to_string(), "disk_base");
    assert!(!ed.is_dirty(), "Tab must remain clean");
    assert!(
        ed.pending_recovery().is_none(),
        "Pending recovery must be None after discard"
    );

    let _ = fs::remove_file(&path);
}

#[test]
fn test_adv_redundant_journal_with_identical_content_is_pruned() {
    let path = unique_temp_path("test_adv_redundant", "txt");
    fs::write(&path, "exact_same_content").unwrap();

    std::thread::sleep(Duration::from_millis(40));
    Journal::record(&path, "exact_same_content").unwrap();
    let jpath = Journal::journal_path(&path);
    assert!(jpath.exists());

    let mut ed = Editor::new();
    ed.open_in_current_tab(&path).unwrap();

    // Redundant journal matches disk content, so it should be pruned automatically
    assert!(
        !jpath.exists(),
        "Redundant journal matching disk content must be deleted"
    );
    assert!(ed.pending_recovery().is_none());
    assert_eq!(ed.buffer().rope().to_string(), "exact_same_content");
    assert!(!ed.is_dirty());

    let _ = fs::remove_file(&path);
}

#[test]
fn test_adv_journal_with_binary_deltas_fresh_and_stale() {
    let path = unique_temp_path("test_adv_deltas", "txt");
    fs::write(&path, "base text").unwrap();

    let jpath = Journal::journal_path(&path);
    let delta = TextDelta {
        start_byte: 9,
        old_text: "".to_string(),
        new_text: " + delta".to_string(),
        timestamp: 100,
    };
    Journal::record_deltas(&path, &[delta]).unwrap();
    assert!(jpath.exists());

    // Recover directly
    let rec = Journal::recover(&path).unwrap().unwrap();
    assert_eq!(rec, "base text + delta");

    // Make disk newer
    std::thread::sleep(Duration::from_millis(40));
    fs::write(&path, "base text updated externally").unwrap();

    let mut ed = Editor::new();
    ed.open_in_current_tab(&path).unwrap();

    // Stale journal deleted
    assert!(!jpath.exists());
    assert_eq!(
        ed.buffer().rope().to_string(),
        "base text updated externally"
    );
    assert!(ed.pending_recovery().is_none());

    let _ = fs::remove_file(&path);
}

#[test]
fn test_adv_physical_journal_deletion_multi_tab_stress() {
    const NUM_TABS: usize = 12;
    let mut paths = Vec::new();
    let mut jpaths = Vec::new();

    for i in 0..NUM_TABS {
        let p = unique_temp_path(&format!("test_adv_multi_{}", i), "txt");
        fs::write(&p, format!("base content {}", i)).unwrap();
        let jp = Journal::journal_path(&p);
        paths.push(p);
        jpaths.push(jp);
    }

    let mut ed = Editor::new();
    ed.open_in_current_tab(&paths[0]).unwrap();
    for p in &paths[1..] {
        ed.open_tab(p).unwrap();
    }
    assert_eq!(ed.tabs().len(), NUM_TABS);

    // Make edits in every tab so each has a physical journal
    for (i, jp) in jpaths.iter().enumerate() {
        ed.switch_tab(i);
        ed.insert_str(&format!(" edit_{}", i)).unwrap();
        ed.flush_journal();
        assert!(
            jp.exists(),
            "Tab {} journal must physically exist on disk after edit",
            i
        );
    }

    // Call discard_all_journals()
    ed.discard_all_journals();

    // Verify EVERY physical journal file has been removed
    for (i, jp) in jpaths.iter().enumerate() {
        assert!(
            !jp.exists(),
            "Tab {} journal {:?} must be physically deleted from disk",
            i,
            jp
        );
        assert!(!Journal::has_journal(&paths[i]));
    }

    for p in paths {
        let _ = fs::remove_file(&p);
    }
}

#[test]
fn test_adv_discard_idempotency_and_boundary_conditions() {
    let mut ed = Editor::new();
    // 1. Discard on empty untitled tab: returns true, doesn't panic
    assert!(ed.discard_tab_changes(0));

    // 2. Discard on out-of-bounds tab index: returns false, doesn't panic
    assert!(!ed.discard_tab_changes(1));
    assert!(!ed.discard_tab_changes(100));

    // 3. Discard when journal file was already deleted externally
    let path = unique_temp_path("test_adv_ext_del", "txt");
    fs::write(&path, "content").unwrap();
    ed.open_in_current_tab(&path).unwrap();
    let jpath = Journal::journal_path(&path);
    assert!(!jpath.exists());
    assert!(ed.discard_tab_changes(0));
    assert!(ed.discard_tab_changes(0)); // Double discard is safe

    // 4. discard_all_journals on clean editor
    ed.discard_all_journals();

    let _ = fs::remove_file(&path);
}

#[test]
fn test_adv_corrupted_journal_does_not_panic() {
    let path = unique_temp_path("test_adv_corrupt", "txt");
    fs::write(&path, "valid disk content").unwrap();

    let jpath = Journal::journal_path(&path);
    // Write corrupted journal with bad header and truncated bytes
    if let Some(parent) = jpath.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut f = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&jpath)
        .unwrap();
    f.write_all(b"LUMJ\x02\xFF\xFF\xFF\xFF").unwrap();
    drop(f);

    let mut ed = Editor::new();
    // Opening should not panic
    let res = ed.open_in_current_tab(&path);
    assert!(res.is_ok(), "Corrupted journal must not prevent safe open");

    let _ = Journal::remove(&path);
    let _ = fs::remove_file(&path);
}

#[test]
fn test_adv_zero_byte_journal_treated_gracefully() {
    let path = unique_temp_path("test_adv_zero_byte", "txt");
    fs::write(&path, "valid disk content").unwrap();

    let jpath = Journal::journal_path(&path);
    if let Some(parent) = jpath.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&jpath, b"").unwrap();

    let mut ed = Editor::new();
    let res = ed.open_in_current_tab(&path);
    assert!(res.is_ok());
    assert_eq!(ed.buffer().rope().to_string(), "valid disk content");

    let _ = Journal::remove(&path);
    let _ = fs::remove_file(&path);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_test_journal_recovery_freshness_invariants(
        disk_content in "[a-zA-Z0-9_\n]{1,64}",
        journal_content in "[a-zA-Z0-9_\n]{1,64}",
        is_fresh in any::<bool>(),
    ) {
        let path = unique_temp_path("prop_rec", "txt");
        fs::write(&path, &disk_content).unwrap();

        if is_fresh {
            std::thread::sleep(Duration::from_millis(20));
            Journal::record(&path, &journal_content).unwrap();
        } else {
            Journal::record(&path, &journal_content).unwrap();
            std::thread::sleep(Duration::from_millis(20));
            fs::write(&path, &disk_content).unwrap();
        }

        let mut ed = Editor::new();
        let open_res = ed.open_in_current_tab(&path);
        prop_assert!(open_res.is_ok());

        let jpath = Journal::journal_path(&path);

        if !is_fresh {
            // Stale journal must be deleted
            prop_assert!(!jpath.exists(), "Stale journal must be removed");
            prop_assert_eq!(ed.buffer().rope().to_string(), disk_content);
            prop_assert!(!ed.is_dirty());
            prop_assert!(ed.pending_recovery().is_none());
        } else if disk_content == journal_content {
            // Redundant journal must be pruned
            prop_assert!(!jpath.exists(), "Redundant journal must be pruned");
            prop_assert_eq!(ed.buffer().rope().to_string(), disk_content);
            prop_assert!(!ed.is_dirty());
            prop_assert!(ed.pending_recovery().is_none());
        } else {
            // Fresh journal with distinct content must be staged
            prop_assert!(jpath.exists(), "Fresh journal must be preserved");
            prop_assert_eq!(ed.buffer().rope().to_string(), disk_content);
            prop_assert!(!ed.is_dirty());
            prop_assert_eq!(ed.pending_recovery(), Some(journal_content.as_str()));

            // Verify discard cleans up physical file
            prop_assert!(ed.discard_tab_changes(0));
            prop_assert!(!jpath.exists(), "Discard must delete physical journal");
            prop_assert!(ed.pending_recovery().is_none());
        }

        let _ = fs::remove_file(&path);
        let _ = Journal::remove(&path);
    }
}
