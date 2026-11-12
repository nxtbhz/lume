//! Adversarial Challenger 2 Tests: TUI Prompts, Discard Lifecycle & Path Sync
//!
//! Stress-tests:
//! 1. PromptAction::ConfirmRecoverJournal handling ('r' vs 'd').
//! 2. Physical .journal deletion on ConfirmCloseTab and ConfirmQuit with Discard.
//! 3. Folder deletion unlinking file_path and clearing journal_path.
//! 4. File rename and directory rename updating journal_path and unlinking old journal.

use std::fs;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::prompt::{InputPrompt, PromptAction};
use crate::tests::TestDir;
use lume_core::journal::Journal;

#[test]
fn test_adv_tui_confirm_recover_journal_prompt_recover_key() {
    let test_dir = TestDir::new("adv_tui_rec");
    let tmp_dir = test_dir.path();
    let file = tmp_dir.join("code.rs");
    fs::write(&file, "fn main() { /* disk */ }\n").unwrap();

    std::thread::sleep(Duration::from_millis(30));
    Journal::record(&file, "fn main() { /* recovered */ }\n").unwrap();

    let mut app = App::from_paths(&[&file]).unwrap();

    // Verify prompt is presented
    match app.prompt {
        Some(InputPrompt {
            action: PromptAction::ConfirmRecoverJournal { tab_idx },
            ..
        }) => {
            assert_eq!(tab_idx, 0);
        }
        ref other => panic!("Expected ConfirmRecoverJournal prompt, got {:?}", other),
    }

    // Press 'r' to recover
    app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE))
        .unwrap();

    // Prompt closed
    assert!(app.prompt.is_none());
    // Content recovered
    assert_eq!(
        app.editor.buffer().rope().to_string(),
        "fn main() { /* recovered */ }\n"
    );
    assert!(app.editor.is_dirty());
    assert!(app.editor.active_tab().pending_recovery.is_none());

    let _ = Journal::remove(&file);
}

#[test]
fn test_adv_tui_confirm_recover_journal_prompt_discard_key() {
    let test_dir = TestDir::new("adv_tui_disc");
    let tmp_dir = test_dir.path();
    let file = tmp_dir.join("discard_me.rs");
    fs::write(&file, "fn main() { /* disk version */ }\n").unwrap();

    std::thread::sleep(Duration::from_millis(30));
    Journal::record(&file, "fn main() { /* unwanted crash version */ }\n").unwrap();
    let jpath = Journal::journal_path(&file);
    assert!(jpath.exists());

    let mut app = App::from_paths(&[&file]).unwrap();
    assert!(app.prompt.is_some());

    // Press 'd' to discard
    app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE))
        .unwrap();

    assert!(app.prompt.is_none());
    // Content remains disk version
    assert_eq!(
        app.editor.buffer().rope().to_string(),
        "fn main() { /* disk version */ }\n"
    );
    assert!(!app.editor.is_dirty());
    assert!(app.editor.active_tab().pending_recovery.is_none());

    // Journal must be physically deleted from disk
    assert!(
        !jpath.exists(),
        "Journal file must be physically deleted after discard key"
    );
    assert!(!Journal::has_journal(&file));
}

#[test]
fn test_adv_tui_confirm_close_tab_discard_deletes_journal() {
    let test_dir = TestDir::new("adv_tui_close_disc");
    let tmp_dir = test_dir.path();
    let file = tmp_dir.join("close_test.rs");
    fs::write(&file, "initial").unwrap();

    let mut app = App::from_paths(&[&file]).unwrap();
    app.editor.insert_str(" dirty edit").unwrap();
    app.editor.flush_journal();
    assert!(app.editor.is_dirty());

    let jpath = Journal::journal_path(&file);
    assert!(
        jpath.exists(),
        "Journal must exist while tab is dirty with edits"
    );

    // Open ConfirmCloseTab prompt
    app.prompt = Some(InputPrompt::confirm_close_tab(0, "close_test.rs"));

    // Press 'd' to discard changes and close
    app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE))
        .unwrap();

    assert!(app.prompt.is_none());
    // Journal file must be deleted physically
    assert!(
        !jpath.exists(),
        "Journal must be physically removed on tab close discard"
    );
    assert!(!Journal::has_journal(&file));
}

#[test]
fn test_adv_tui_confirm_quit_discard_deletes_all_journals() {
    let test_dir = TestDir::new("adv_tui_quit_disc");
    let tmp_dir = test_dir.path();
    let file1 = tmp_dir.join("q1.rs");
    let file2 = tmp_dir.join("q2.rs");
    fs::write(&file1, "f1").unwrap();
    fs::write(&file2, "f2").unwrap();

    let mut app = App::from_paths(&[&file1, &file2]).unwrap();
    app.editor.switch_tab(0);
    app.editor.insert_str(" dirt1").unwrap();
    app.editor.switch_tab(1);
    app.editor.insert_str(" dirt2").unwrap();
    app.editor.flush_journal();

    let jp1 = Journal::journal_path(&file1);
    let jp2 = Journal::journal_path(&file2);
    assert!(jp1.exists());
    assert!(jp2.exists());

    // Open ConfirmQuit prompt
    app.prompt = Some(InputPrompt::confirm_quit());

    // Press 'd' to discard all and quit
    app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE))
        .unwrap();

    assert!(!app.running, "App must set running = false on quit discard");
    assert!(
        !jp1.exists(),
        "File 1 journal must be physically deleted on quit discard"
    );
    assert!(
        !jp2.exists(),
        "File 2 journal must be physically deleted on quit discard"
    );
}

#[test]
fn test_adv_tui_folder_deletion_clears_journal_path_and_unlinks() {
    let test_dir = TestDir::new("adv_tui_rmdir");
    let tmp_dir = test_dir.path();
    let sub = tmp_dir.join("target_dir");
    fs::create_dir_all(&sub).unwrap();
    let file = sub.join("inner.rs");
    fs::write(&file, "fn inner() {}").unwrap();

    let mut app = App::from_paths(&[tmp_dir, &file]).unwrap();
    assert_eq!(app.editor.tabs().len(), 1);

    let canon_file = file.canonicalize().unwrap();
    assert_eq!(
        app.editor.active_tab().journal_path,
        Some(Journal::journal_path(&canon_file))
    );

    // Record journal
    Journal::record(&canon_file, "unsaved inner").unwrap();
    let jp = Journal::journal_path(&canon_file);
    assert!(jp.exists());

    // Trigger handle_deleted_folder_tabs
    let canon_sub = sub.canonicalize().unwrap();
    app.handle_deleted_folder_tabs(&canon_sub);

    // Assert tab is unlinked to untitled
    assert!(
        app.editor.file_path().is_none(),
        "Tab file_path must be cleared to None"
    );
    assert!(
        app.editor.active_tab().journal_path.is_none(),
        "Tab journal_path must be cleared to None"
    );
    assert!(
        app.editor.is_dirty(),
        "Tab must be marked dirty to avoid losing buffer contents"
    );

    // Assert journal is physically deleted from disk
    assert!(
        !jp.exists(),
        "Journal must be removed when enclosing folder is deleted"
    );
}

#[test]
fn test_adv_tui_deep_folder_deletion_clears_descendant_tabs() {
    let test_dir = TestDir::new("adv_tui_deep_rmdir");
    let tmp_dir = test_dir.path();
    let deep_dir = tmp_dir.join("a").join("b").join("c");
    fs::create_dir_all(&deep_dir).unwrap();
    let file1 = deep_dir.join("deep1.rs");
    let file2 = deep_dir.join("deep2.rs");
    fs::write(&file1, "fn d1() {}").unwrap();
    fs::write(&file2, "fn d2() {}").unwrap();

    let mut app = App::from_paths(&[tmp_dir, &file1, &file2]).unwrap();
    assert_eq!(app.editor.tabs().len(), 2);

    let c1 = file1.canonicalize().unwrap();
    let c2 = file2.canonicalize().unwrap();
    Journal::record(&c1, "j1").unwrap();
    Journal::record(&c2, "j2").unwrap();

    // Delete top-level folder 'a'
    let folder_a = tmp_dir.join("a").canonicalize().unwrap();
    app.handle_deleted_folder_tabs(&folder_a);

    // Both tabs must be cleared
    assert!(app.editor.tabs()[0].file_path.is_none());
    assert!(app.editor.tabs()[0].journal_path.is_none());
    assert!(app.editor.tabs()[1].file_path.is_none());
    assert!(app.editor.tabs()[1].journal_path.is_none());

    // Both journals deleted
    assert!(!Journal::has_journal(&c1));
    assert!(!Journal::has_journal(&c2));
}

#[test]
fn test_adv_tui_folder_deletion_isolation() {
    let test_dir = TestDir::new("adv_tui_isolation");
    let tmp_dir = test_dir.path();
    let dir1 = tmp_dir.join("dir1");
    let dir2 = tmp_dir.join("dir2");
    fs::create_dir_all(&dir1).unwrap();
    fs::create_dir_all(&dir2).unwrap();

    let file1 = dir1.join("f1.rs");
    let file2 = dir2.join("f2.rs");
    fs::write(&file1, "f1").unwrap();
    fs::write(&file2, "f2").unwrap();

    let mut app = App::from_paths(&[tmp_dir, &file1, &file2]).unwrap();
    let c1 = file1.canonicalize().unwrap();
    let c2 = file2.canonicalize().unwrap();

    Journal::record(&c1, "j1").unwrap();
    Journal::record(&c2, "j2").unwrap();
    let jp1 = Journal::journal_path(&c1);
    let jp2 = Journal::journal_path(&c2);

    // Delete ONLY dir1
    let canon_dir1 = dir1.canonicalize().unwrap();
    app.handle_deleted_folder_tabs(&canon_dir1);

    // Tab 0 (dir1) must be cleared
    assert!(app.editor.tabs()[0].file_path.is_none());
    assert!(app.editor.tabs()[0].journal_path.is_none());
    assert!(!jp1.exists());

    // Tab 1 (dir2) must be completely INTACT
    assert_eq!(app.editor.tabs()[1].file_path, Some(c2.clone()));
    assert_eq!(app.editor.tabs()[1].journal_path, Some(jp2.clone()));
    assert!(jp2.exists());

    let _ = Journal::remove(&c2);
}

#[test]
fn test_adv_tui_file_rename_synchronizes_journal_path() {
    let test_dir = TestDir::new("adv_tui_rename_file");
    let tmp_dir = test_dir.path();
    let old_file = tmp_dir.join("old_name.rs");
    let new_file = tmp_dir.join("new_name.rs");
    fs::write(&old_file, "fn original() {}").unwrap();

    let mut app = App::from_paths(&[tmp_dir, &old_file]).unwrap();
    let old_canon = old_file.canonicalize().unwrap();

    // Create journal for old path
    Journal::record(&old_canon, "dirty before rename").unwrap();
    let old_jp = Journal::journal_path(&old_canon);
    assert!(old_jp.exists());

    // Rename file on disk
    fs::rename(&old_file, &new_file).unwrap();
    let new_canon = new_file.canonicalize().unwrap();

    // Update app via handle_renamed_path_tabs
    app.handle_renamed_path_tabs(&old_file, &new_file);

    // Assert tab.file_path and tab.journal_path are updated to new path
    assert_eq!(app.editor.active_tab().file_path, Some(new_canon.clone()));
    assert_eq!(
        app.editor.active_tab().journal_path,
        Some(Journal::journal_path(&new_canon))
    );

    // Old journal must be physically deleted
    assert!(
        !old_jp.exists(),
        "Old journal must be removed when file is renamed"
    );
    assert!(!Journal::has_journal(&old_canon));
}

#[test]
fn test_adv_tui_folder_rename_updates_subpath_tab_journal_paths() {
    let test_dir = TestDir::new("adv_tui_rename_folder");
    let tmp_dir = test_dir.path();
    let old_folder = tmp_dir.join("old_module");
    let new_folder = tmp_dir.join("new_module");
    fs::create_dir_all(&old_folder).unwrap();

    let old_sub_file = old_folder.join("helper.rs");
    fs::write(&old_sub_file, "fn helper() {}").unwrap();

    let mut app = App::from_paths(&[tmp_dir, &old_sub_file]).unwrap();
    let old_file_canon = old_sub_file.canonicalize().unwrap();
    Journal::record(&old_file_canon, "helper dirty").unwrap();
    let old_jp = Journal::journal_path(&old_file_canon);
    assert!(old_jp.exists());

    // Rename directory on disk
    fs::rename(&old_folder, &new_folder).unwrap();
    let new_folder_canon = new_folder.canonicalize().unwrap();
    let expected_new_file = new_folder_canon.join("helper.rs");

    // Update app
    app.handle_renamed_path_tabs(&old_folder, &new_folder);

    // Verify subpath was synchronized
    assert_eq!(
        app.editor.active_tab().file_path,
        Some(expected_new_file.clone())
    );
    assert_eq!(
        app.editor.active_tab().journal_path,
        Some(Journal::journal_path(&expected_new_file))
    );

    // Old journal removed
    assert!(!old_jp.exists());
    assert!(!Journal::has_journal(&old_file_canon));
}
