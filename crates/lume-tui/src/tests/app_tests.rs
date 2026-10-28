use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{ActiveFocus, App};
use crate::tests::TestDir;
use lume_core::TextBuffer;

#[test]
fn test_app_open_directory_workspace() {
    let test_dir = TestDir::new("lume_test_ws");
    let tmp_dir = test_dir.path();
    let sub_file = tmp_dir.join("workspace_sample.txt");
    std::fs::write(&sub_file, "sample in ws").unwrap();

    let app = App::from_paths(&[tmp_dir]).unwrap();
    assert_eq!(app.explorer.root, std::fs::canonicalize(tmp_dir).unwrap());
    // Editor should have 1 untitled clean tab
    assert_eq!(app.editor.tabs().len(), 1);
    assert!(app.editor.file_path().is_none());
}

#[test]
fn test_app_open_single_existing_file() {
    let app = App::from_paths(&["Cargo.toml"]).unwrap();
    assert_eq!(app.editor.tabs().len(), 1);
    assert!(app.editor.file_path().is_some());
    assert!(app.editor.buffer().len_bytes() > 0);
}

#[test]
fn test_app_open_single_new_file() {
    let new_file_name = format!("new_file_{}.txt", std::process::id());
    let app = App::from_paths(&[&new_file_name]).unwrap();
    assert_eq!(app.editor.tabs().len(), 1);
    assert!(app.editor.file_path().is_some());
    assert_eq!(app.editor.buffer().len_bytes(), 0);
}

#[test]
fn test_app_open_multiple_files_tabs() {
    let test_dir = TestDir::new("lume_multi");
    let tmp_dir = test_dir.path();
    let file1 = tmp_dir.join("file1.txt");
    let file2 = tmp_dir.join("file2.txt");
    std::fs::write(&file1, "file1 content").unwrap();
    std::fs::write(&file2, "file2 content").unwrap();

    let app = App::from_paths(&[&file1, &file2]).unwrap();
    assert_eq!(app.editor.tabs().len(), 2);
    assert_eq!(app.editor.active_tab_idx(), 0);

    // Tab 1
    assert_eq!(
        app.editor
            .buffer()
            .slice_to_cow(0, app.editor.buffer().len_bytes()),
        "file1 content"
    );
}

#[test]
fn test_app_open_directory_and_file() {
    let test_dir = TestDir::new("lume_ws_file");
    let tmp_dir = test_dir.path();
    let file = tmp_dir.join("hello.txt");
    std::fs::write(&file, "hello world").unwrap();

    let app = App::from_paths(&[tmp_dir, &file]).unwrap();
    assert_eq!(app.explorer.root, std::fs::canonicalize(tmp_dir).unwrap());
    assert_eq!(app.editor.tabs().len(), 1);
    assert_eq!(
        app.editor
            .buffer()
            .slice_to_cow(0, app.editor.buffer().len_bytes()),
        "hello world"
    );
}

#[test]
fn test_app_open_wildcard_glob() {
    let test_dir = TestDir::new("lume_glob");
    let tmp_dir = test_dir.path();
    let f1 = tmp_dir.join("test_a.txt");
    let f2 = tmp_dir.join("test_b.txt");
    std::fs::write(&f1, "content a").unwrap();
    std::fs::write(&f2, "content b").unwrap();

    let pattern = format!("{}/*.txt", tmp_dir.display());
    let app = App::from_paths(&[&pattern]).unwrap();
    assert_eq!(app.editor.tabs().len(), 2);
    assert_eq!(app.editor.active_tab_idx(), 0);
}

#[test]
fn test_app_explorer_create_nested_folder_via_prompt() {
    let test_dir = TestDir::new("lume_app_mkdir");
    let tmp_dir = test_dir.path();

    let mut app = App::from_paths(&[tmp_dir]).unwrap();
    app.focus = ActiveFocus::Explorer;

    // Press 'n' in Explorer to open new folder prompt
    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
        .unwrap();
    assert!(app.prompt.is_some());

    // Type nested folder name 'deep/nested/pkg'
    for c in "deep/nested/pkg".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }

    // Press Enter to submit
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert!(app.prompt.is_none());

    let created_path = app.explorer.root.join("deep/nested/pkg");
    assert!(created_path.exists());
    assert!(created_path.is_dir());
}

#[test]
fn test_app_explorer_delete_folder_and_unlinks_open_tab() {
    let test_dir = TestDir::new("lume_app_rmdir");
    let tmp_dir = test_dir.path();

    let folder = tmp_dir.join("submodule");
    std::fs::create_dir_all(&folder).unwrap();
    let sub_file = folder.join("file.rs");
    std::fs::write(&sub_file, "fn test() {}").unwrap();

    let mut app = App::from_paths(&[tmp_dir, &sub_file]).unwrap();
    assert_eq!(app.editor.tabs().len(), 1);
    let editor_path = app.editor.file_path().unwrap();
    assert_eq!(
        editor_path.canonicalize().unwrap(),
        sub_file.canonicalize().unwrap()
    );

    app.focus = ActiveFocus::Explorer;
    // Select 'submodule' directory in explorer
    if let Some(pos) = app
        .explorer
        .items
        .iter()
        .position(|it| it.name == "submodule")
    {
        app.explorer.selected_idx = pos;
    }

    // Press 'd' to open delete prompt
    app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE))
        .unwrap();
    assert!(app.prompt.is_some());

    // Confirm delete with Enter
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert!(app.prompt.is_none());

    // Folder is deleted on disk
    assert!(!folder.exists());

    // Open tab for file inside deleted directory must have its file_path unlinked
    assert!(app.editor.file_path().is_none());
}

#[test]
fn test_delete_folder_unlinks_relative_path_tabs_and_cleans_journal() {
    let test_dir = TestDir::new("lume_rel_unlink");
    let tmp_dir = test_dir.path();

    let sub_folder = tmp_dir.join("pkg");
    std::fs::create_dir_all(&sub_folder).unwrap();
    let sub_file = sub_folder.join("lib.rs");
    std::fs::write(&sub_file, "pub fn hello() {}").unwrap();

    let mut app = App::from_paths(&[tmp_dir]).unwrap();
    // Open tab with a relative path: "pkg/lib.rs"
    let rel_file = PathBuf::from("pkg/lib.rs");
    app.editor.open_or_load(tmp_dir.join(&rel_file)).unwrap();
    // Explicitly set relative path on tab to test relative unlinking robustness
    app.editor.set_file_path(Some(rel_file.clone()));
    // Create crash journal for it
    let canon_file = std::fs::canonicalize(&sub_file).unwrap();
    lume_core::Journal::record(&canon_file, "unsaved content").unwrap();
    assert!(lume_core::Journal::has_journal(&canon_file));

    app.focus = ActiveFocus::Explorer;
    let pkg_canon = std::fs::canonicalize(&sub_folder).unwrap();
    // Delete folder via command / app
    app.handle_deleted_folder_tabs(&pkg_canon);
    app.explorer.delete_folder(&pkg_canon).unwrap();

    // The relative tab must be unlinked
    assert!(app.editor.file_path().is_none());
    assert!(app.editor.is_dirty());
    // Journal must have been removed
    assert!(!lume_core::Journal::has_journal(&canon_file));
}

#[test]
fn test_explorer_ignores_ctrl_modifiers_on_char_actions() {
    let mut app = App::new(None).unwrap();
    app.focus = ActiveFocus::Explorer;

    // Ctrl+A should NOT open new folder prompt
    app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(
        app.prompt.is_none(),
        "Ctrl+A should not open prompt in explorer"
    );

    // Ctrl+D should NOT open delete prompt
    app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(
        app.prompt.is_none(),
        "Ctrl+D should not open prompt in explorer"
    );
}

#[test]
fn test_app_smart_indent_on_enter() {
    let mut app = App::new(None).unwrap();
    app.editor.insert_str("    fn test() {").unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert_eq!(
        app.editor
            .buffer()
            .slice_to_cow(0, app.editor.buffer().len_bytes()),
        "    fn test() {\n        "
    );
    assert_eq!(app.editor.cursor_line_col(), (1, 8));
}

#[test]
fn test_app_smart_unindent_on_backspace() {
    let mut app = App::new(None).unwrap();
    app.editor.insert_str("        ").unwrap();
    assert_eq!(app.editor.cursor_line_col(), (0, 8));

    app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
        .unwrap();
    assert_eq!(
        app.editor
            .buffer()
            .slice_to_cow(0, app.editor.buffer().len_bytes()),
        "    "
    );
    assert_eq!(app.editor.cursor_line_col(), (0, 4));
}

#[test]
fn test_app_auto_pairs_and_overtype() {
    let mut app = App::new(None).unwrap();
    // Type '(' -> inserts '()'
    app.handle_key(KeyEvent::new(KeyCode::Char('('), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(
        app.editor
            .buffer()
            .slice_to_cow(0, app.editor.buffer().len_bytes()),
        "()"
    );
    assert_eq!(app.editor.cursor_line_col(), (0, 1));

    // Type ')' -> overtypes
    app.handle_key(KeyEvent::new(KeyCode::Char(')'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(
        app.editor
            .buffer()
            .slice_to_cow(0, app.editor.buffer().len_bytes()),
        "()"
    );
    assert_eq!(app.editor.cursor_line_col(), (0, 2));
}

#[test]
fn test_app_toggle_settings_commands() {
    let mut app = App::new(None).unwrap();

    // Toggle Auto-Pairs
    assert!(app.editor.config().auto_pairs);
    app.execute_command("settings.toggle_auto_pairs");
    assert!(!app.editor.config().auto_pairs);
    assert_eq!(
        app.status_message.as_ref().map(|(s, _)| s.as_str()),
        Some("Auto-closing pairs: Disabled")
    );

    app.execute_command("settings.toggle_auto_pairs");
    assert!(app.editor.config().auto_pairs);
    assert_eq!(
        app.status_message.as_ref().map(|(s, _)| s.as_str()),
        Some("Auto-closing pairs: Enabled")
    );

    // Toggle Smart Indent
    assert!(app.editor.config().smart_indent);
    app.execute_command("settings.toggle_smart_indent");
    assert!(!app.editor.config().smart_indent);
    assert_eq!(
        app.status_message.as_ref().map(|(s, _)| s.as_str()),
        Some("Smart indentation: Disabled")
    );

    // Toggle Smart Unindent
    assert!(app.editor.config().smart_unindent);
    app.execute_command("settings.toggle_smart_unindent");
    assert!(!app.editor.config().smart_unindent);
    assert_eq!(
        app.status_message.as_ref().map(|(s, _)| s.as_str()),
        Some("Smart unindent: Disabled")
    );
}

#[test]
fn test_app_bracket_matching_ctrl_m() {
    let mut app = App::new(None).unwrap();
    app.editor.insert_str("fn main() { return; }").unwrap();
    // Index of '(' is 7, ')' is 8
    app.editor.cursors_mut().set_single(7);

    // Press Ctrl+M
    app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(app.editor.cursors().primary().head, 8);
    assert_eq!(
        app.status_message.as_ref().map(|(s, _)| s.as_str()),
        Some("Jumped to matching bracket")
    );

    // Press Ctrl+M again: jumps back to '('
    app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(app.editor.cursors().primary().head, 7);
}

#[test]
fn test_app_bracket_matching_command_palette() {
    let mut app = App::new(None).unwrap();
    app.editor.insert_str("{ let a = 1; }").unwrap();
    // Index of '{' is 0, '}' is 13
    app.editor.cursors_mut().set_single(0);

    app.execute_command("navigation.match_bracket");
    assert_eq!(app.editor.cursors().primary().head, 13);
    assert_eq!(
        app.status_message.as_ref().map(|(s, _)| s.as_str()),
        Some("Jumped to matching bracket")
    );

    // If cursor has no matching bracket
    let mut app_none = App::new(None).unwrap();
    app_none.editor.insert_str("hello").unwrap();
    app_none.editor.cursors_mut().set_single(0);
    app_none.execute_command("navigation.match_bracket");
    assert_eq!(
        app_none.status_message.as_ref().map(|(s, _)| s.as_str()),
        Some("No matching bracket found")
    );
}

#[test]
fn test_app_bracket_matching_render_highlight() {
    let mut app = App::new(None).unwrap();
    app.editor.insert_str("fn test() {}").unwrap();
    // Place cursor on '(' (offset 7)
    app.editor.cursors_mut().set_single(7);

    let mut out = Vec::new();
    app.render(&mut out).unwrap();
    let rendered = String::from_utf8_lossy(&out);

    // Should contain 48;2;70;70;110 (the RGB escape sequence for matching bracket highlight)
    assert!(
        rendered.contains("48;2;70;70;110"),
        "Render output should contain bracket matching highlight RGB 70;70;110"
    );
}

#[test]
fn test_app_key_navigation_and_tab_shortcuts() {
    let mut app = App::new(None).unwrap();
    app.editor.insert_str("line 1\nline 2\nline 3").unwrap();
    app.editor.cursors_mut().set_single(0);

    // Arrow keys
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE))
        .unwrap();

    // Shift + Arrow keys (Selection)
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT))
        .unwrap();
    assert!(app.editor.cursors().primary().is_selection());
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE))
        .unwrap(); // collapses
    assert!(!app.editor.cursors().primary().is_selection());

    // Tab navigation shortcuts
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(app.editor.tabs().len(), 2);

    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::CONTROL))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT))
        .unwrap();

    app.handle_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(app.editor.tabs().len(), 1);

    // Open Which-Key via F1, render, and close via Esc
    app.handle_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE))
        .unwrap();
    assert!(app.which_key_open);
    let mut buf = Vec::new();
    assert!(app.render(&mut buf).is_ok());
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .unwrap();
    assert!(!app.which_key_open);

    // Toggle explorer via F2
    assert!(app.explorer.is_visible);
    app.handle_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE))
        .unwrap();
    assert!(!app.explorer.is_visible);
    app.handle_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE))
        .unwrap();
    assert!(app.explorer.is_visible);
}

#[test]
fn test_app_explorer_keyboard_navigation() {
    let mut app = App::new(None).unwrap();
    app.focus = ActiveFocus::Explorer;
    assert!(app.explorer.is_visible);

    // Explorer key events
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE))
        .unwrap();

    // Switch focus back to Editor
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.focus, ActiveFocus::Editor);
}

#[test]
fn test_handle_renamed_file_tab_path_and_journal_sync() {
    let test_dir = TestDir::new("lume_test_rename_tab_sync");
    let old_file = test_dir.path().join("script.py");
    let new_file = test_dir.path().join("script.rs");
    std::fs::write(&old_file, "print('hello')").unwrap();

    let mut app = App::new(Some(test_dir.path().to_path_buf())).unwrap();
    app.editor.open_or_load(&old_file).unwrap();
    assert_eq!(app.editor.active_tab().file_path.as_ref().unwrap(), &old_file.canonicalize().unwrap());
    assert_eq!(app.current_language().map(|l| l.name()), Some("Python"));

    // Simulate journal creation
    let _ = lume_core::Journal::record(&old_file, "unsaved changes");
    assert!(lume_core::Journal::has_journal(&old_file));

    // Execute rename tab sync
    app.handle_renamed_path_tabs(&old_file, &new_file);

    let active = app.editor.active_tab();
    let new_canon = new_file.canonicalize().unwrap_or(new_file.clone());
    assert_eq!(active.file_path.as_ref().unwrap(), &new_canon);
    assert_eq!(app.current_language().map(|l| l.name()), Some("Rust"));
    assert!(!lume_core::Journal::has_journal(&old_file));
}

#[test]
fn test_handle_renamed_folder_tabs_subpath_and_language_sync() {
    let test_dir = TestDir::new("lume_test_rename_folder_tab_sync");
    let old_dir = test_dir.path().join("pkg_old");
    let new_dir = test_dir.path().join("pkg_new");
    std::fs::create_dir_all(&old_dir).unwrap();
    let file1 = old_dir.join("module.rs");
    std::fs::write(&file1, "pub fn run() {}").unwrap();

    let mut app = App::new(Some(test_dir.path().to_path_buf())).unwrap();
    app.editor.open_or_load(&file1).unwrap();

    // Renaming directory should update nested open tab path
    app.handle_renamed_path_tabs(&old_dir, &new_dir);

    let active = app.editor.active_tab();
    assert_eq!(active.file_path.as_ref().unwrap().file_name().unwrap(), "module.rs");
    assert!(active.file_path.as_ref().unwrap().starts_with(&new_dir) || active.file_path.as_ref().unwrap().ends_with("pkg_new/module.rs"));
}
