//! Challenger Milestone 1 Empirical Stress Tests (TUI)
//!
//! Tests:
//! 4. Tab bar [x] mouse click dirty guard:
//!    - Dirty tab click on [x] opens ConfirmCloseTab prompt, tab not closed
//!    - Clean tab click on [x] immediately closes tab, prompt is None
//!    - Click on tab title switches tab without prompt
//!    - Multi-tab scenario: tab 0 dirty, tab 1 dirty, tab 2 clean. Clicking tab 1's [x] targets tab 1.
//! 5. Autosave FocusLost guard:
//!    - When autosave.enabled is false (default), FocusLost does NOT save to disk
//!    - When autosave.enabled is true, FocusLost saves to disk
//!    - Untitled tab with FocusLost does not crash and does not write
//! 6. Tab state non-mutation on open failure in App:
//!    - Opening binary/invalid file leaves tab untitled, clean, and sets status bar error

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::fs;
use std::path::PathBuf;

use crate::app::App;
use crate::input::AppEvent;
use crate::prompt::PromptAction;
use lume_core::TextBuffer;

fn temp_file(prefix: &str, ext: &str) -> PathBuf {
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
// 4. Tab Bar [x] Mouse Click Dirty Guard Stress Tests
// -----------------------------------------------------------------------------

#[test]
fn test_tab_bar_x_click_on_dirty_tab_opens_prompt_and_preserves_tab() {
    let mut app = App::new(None).unwrap();
    app.editor.insert_str("dirty work").unwrap();
    assert!(app.editor.is_dirty());
    assert_eq!(app.editor.tabs().len(), 1);

    // Tab 0 is active and dirty.
    // tab_text: " [ 1: [Untitled]* x ] "
    // Find the exact 'x' column in tab text
    let tab = &app.editor.tabs()[0];
    let dirty = "*";
    let tab_text = format!(" [ 1: {}{} x ] ", tab.name(), dirty);
    let x_col = tab_text.find('x').expect("tab text must contain 'x'");

    // Simulate mouse click directly on the tab bar at the 'x' button
    app.handle_tab_bar_click(x_col);

    // 1. Tab must NOT be closed
    assert_eq!(
        app.editor.tabs().len(),
        1,
        "Dirty tab must not be closed on [x] click"
    );
    assert!(app.editor.is_dirty());

    // 2. Prompt must be opened with ConfirmCloseTab
    assert!(app.prompt.is_some(), "Prompt modal must be displayed");
    let prompt = app.prompt.as_ref().unwrap();
    match prompt.action {
        PromptAction::ConfirmCloseTab { tab_idx } => {
            assert_eq!(tab_idx, 0, "ConfirmCloseTab must target tab 0");
        }
        _ => panic!(
            "Expected PromptAction::ConfirmCloseTab, got {:?}",
            prompt.action
        ),
    }
}

#[test]
fn test_tab_bar_x_click_via_mouse_event_on_dirty_tab() {
    let mut app = App::new(None).unwrap();
    app.editor.insert_str("content").unwrap();
    assert!(app.editor.is_dirty());

    let offset = app.text_area_left();
    let tab = &app.editor.tabs()[0];
    let tab_text = format!(" [ 1: {}* x ] ", tab.name());
    let x_col = tab_text.find('x').unwrap();

    // Click via handle_mouse
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: (offset + x_col) as u16,
        row: 0,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();

    assert_eq!(app.editor.tabs().len(), 1);
    assert!(app.prompt.is_some());
    assert!(matches!(
        app.prompt.as_ref().unwrap().action,
        PromptAction::ConfirmCloseTab { tab_idx: 0 }
    ));
}

#[test]
fn test_tab_bar_x_click_on_clean_tab_closes_immediately() {
    let mut app = App::new(None).unwrap();
    // Add a second clean tab so closing one leaves one
    app.editor.new_tab();
    assert_eq!(app.editor.tabs().len(), 2);
    assert_eq!(app.editor.active_tab_idx(), 1);
    assert!(!app.editor.tabs()[1].is_dirty);

    // Tab 1 is active and clean
    let tab = &app.editor.tabs()[1];
    let tab0_len = format!("  1: {} x   ", app.editor.tabs()[0].name())
        .chars()
        .count();
    let tab1_text = format!(" [ 2: {} x ] ", tab.name());
    let x_col_relative = tab1_text.find('x').unwrap();
    let x_col_global = tab0_len + x_col_relative;

    // Click 'x' on clean tab 1
    app.handle_tab_bar_click(x_col_global);

    // Tab 1 must be closed immediately without prompt
    assert_eq!(
        app.editor.tabs().len(),
        1,
        "Clean tab must close immediately"
    );
    assert!(
        app.prompt.is_none(),
        "No prompt should appear for clean tab close"
    );
    assert_eq!(
        app.status_message.as_ref().map(|s| s.0.as_str()),
        Some("Closed tab")
    );
}

#[test]
fn test_tab_bar_click_on_title_switches_tab_without_closing() {
    let mut app = App::new(None).unwrap();
    app.editor.insert_str("tab 1 dirty").unwrap();
    app.editor.new_tab();
    app.editor.insert_str("tab 2").unwrap();

    assert_eq!(app.editor.active_tab_idx(), 1);

    // Click on tab 0 title (col 3 is within "  1: [Untitled]* x   ")
    app.handle_tab_bar_click(3);

    // Tab should switch to 0, not close, and no prompt
    assert_eq!(app.editor.active_tab_idx(), 0);
    assert_eq!(app.editor.tabs().len(), 2);
    assert!(app.prompt.is_none());
}

#[test]
fn test_tab_bar_multi_tab_dirty_guard_targeting() {
    let mut app = App::new(None).unwrap();
    // Tab 0: dirty
    app.editor.insert_str("tab 0 content").unwrap();
    // Tab 1: dirty
    app.editor.new_tab();
    app.editor.insert_str("tab 1 content").unwrap();
    // Tab 2: clean
    app.editor.new_tab();

    assert_eq!(app.editor.tabs().len(), 3);
    assert!(app.editor.tabs()[0].is_dirty);
    assert!(app.editor.tabs()[1].is_dirty);
    assert!(!app.editor.tabs()[2].is_dirty);

    // Compute column for Tab 1's [x] button
    // Tab 0 is inactive: "  1: [Untitled]* x   "
    let tab0_str = format!("  1: {}* x   ", app.editor.tabs()[0].name());
    let tab0_len = tab0_str.chars().count();

    // Tab 1 is inactive (active is 2): "  2: [Untitled]* x   "
    let tab1_str = format!("  2: {}* x   ", app.editor.tabs()[1].name());
    let tab1_x_offset = tab1_str.find('x').unwrap();

    let tab1_x_global = tab0_len + tab1_x_offset;

    // Click [x] on Tab 1
    app.handle_tab_bar_click(tab1_x_global);

    // Tab 1 is dirty -> prompt must target Tab 1
    assert_eq!(app.editor.tabs().len(), 3, "Dirty tab 1 must not be closed");
    assert!(app.prompt.is_some());
    match app.prompt.as_ref().unwrap().action {
        PromptAction::ConfirmCloseTab { tab_idx } => {
            assert_eq!(tab_idx, 1, "ConfirmCloseTab must specifically target tab 1");
        }
        _ => panic!("Expected ConfirmCloseTab"),
    }
}

// -----------------------------------------------------------------------------
// 5. Autosave FocusLost Guard Stress Tests
// -----------------------------------------------------------------------------

#[test]
fn test_autosave_focus_lost_disabled_by_default_does_not_write_to_disk() {
    let path = temp_file("chal_autosave_off", "txt");
    fs::write(&path, "original disk text").unwrap();

    let mut app = App::from_paths(&[&path]).unwrap();
    // Confirm default autosave config is disabled
    assert!(
        !app.editor.autosave_controller().config.enabled,
        "Autosave must be disabled by default"
    );

    // Modify buffer
    app.editor.insert_str(" MODIFIED").unwrap();
    assert!(app.editor.is_dirty());

    let mut sink = Vec::new();
    // Dispatch AppEvent::FocusLost
    app.handle_event(AppEvent::FocusLost, &mut sink).unwrap();

    // Give any background thread opportunity to execute
    std::thread::sleep(std::time::Duration::from_millis(50));
    let _ = app.handle_tick(&mut sink);

    // Read file from disk: MUST NOT HAVE CHANGED!
    let disk_content = fs::read_to_string(&path).unwrap();
    let _ = fs::remove_file(&path);

    assert_eq!(
        disk_content, "original disk text",
        "FocusLost MUST NOT write to disk when autosave is disabled"
    );
    assert!(app.editor.is_dirty(), "Tab must remain dirty");
}

#[test]
fn test_autosave_focus_lost_when_enabled_saves_to_disk() {
    let path = temp_file("chal_autosave_on", "txt");
    fs::write(&path, "original disk text").unwrap();

    let mut app = App::from_paths(&[&path]).unwrap();
    // Explicitly enable autosave
    app.editor.autosave_controller_mut().config.enabled = true;

    // Modify buffer
    app.editor.insert_str(" MODIFIED").unwrap();
    assert!(app.editor.is_dirty());

    let mut sink = Vec::new();
    // Dispatch AppEvent::FocusLost
    app.handle_event(AppEvent::FocusLost, &mut sink).unwrap();

    // Poll async saves until done
    for _ in 0..20 {
        app.handle_tick(&mut sink).unwrap();
        if !app.editor.is_dirty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    let disk_content = fs::read_to_string(&path).unwrap();
    let _ = fs::remove_file(&path);

    assert!(
        disk_content.contains("MODIFIED"),
        "FocusLost MUST save to disk when autosave is enabled"
    );
    assert!(!app.editor.is_dirty(), "Tab must be clean after save");
}

#[test]
fn test_autosave_focus_lost_on_untitled_tab_is_safe_noop() {
    let mut app = App::new(None).unwrap();
    app.editor.autosave_controller_mut().config.enabled = true;
    app.editor.insert_str("untitled dirty work").unwrap();
    assert!(app.editor.is_dirty());
    assert!(app.editor.file_path().is_none());

    let mut sink = Vec::new();
    // FocusLost on untitled dirty tab must not panic or error
    let res = app.handle_event(AppEvent::FocusLost, &mut sink);
    assert!(res.is_ok());
    assert!(app.editor.is_dirty());
}

#[test]
fn test_app_open_binary_file_retains_untitled_clean_tab_with_status_error() {
    let bad_path = temp_file("chal_app_bad_open", "bin");
    fs::write(&bad_path, [0x00, 0x01, 0x02]).unwrap();

    let app = App::from_paths(&[&bad_path]).unwrap();
    let _ = fs::remove_file(&bad_path);

    // Active tab must be untitled and clean
    assert_eq!(app.editor.tabs().len(), 1);
    assert_eq!(
        app.editor.file_path(),
        None,
        "Tab file_path must be None on open failure"
    );
    assert!(
        !app.editor.is_dirty(),
        "Tab must not be dirty on open failure"
    );
    assert_eq!(app.editor.buffer().len_bytes(), 0);

    // Status bar must display error message
    assert!(app.status_message.is_some(), "Status bar must show error");
    let (status_text, _) = app.status_message.unwrap();
    assert!(
        status_text.contains("Error opening"),
        "Status message: {}",
        status_text
    );
    assert!(
        status_text.contains("Binary file detected"),
        "Status message: {}",
        status_text
    );
}
