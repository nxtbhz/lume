use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{ActiveFocus, App};
use crate::prompt::{InputPrompt, PromptAction};
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
    assert_eq!(
        app.editor.active_tab().file_path.as_ref().unwrap(),
        &old_file.canonicalize().unwrap()
    );
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
    assert_eq!(
        active.file_path.as_ref().unwrap().file_name().unwrap(),
        "module.rs"
    );
    assert!(
        active.file_path.as_ref().unwrap().starts_with(&new_dir)
            || active
                .file_path
                .as_ref()
                .unwrap()
                .ends_with("pkg_new/module.rs")
    );
}

#[test]
fn test_app_explorer_create_file_via_prompt_opens_editor_tab() {
    let test_dir = TestDir::new("lume_test_app_create_file");
    let mut app = App::new(Some(test_dir.path().to_path_buf())).unwrap();
    app.focus = ActiveFocus::Explorer;

    // Press 'a' to open new file prompt
    app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE))
        .unwrap();
    assert!(app.prompt.is_some());
    assert!(matches!(
        app.prompt.as_ref().unwrap().action,
        PromptAction::CreateFile { .. }
    ));

    // Type filename "main.rs" and press Enter
    for c in "main.rs".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();

    assert!(app.prompt.is_none());
    assert_eq!(app.focus, ActiveFocus::Editor);
    assert_eq!(
        app.editor
            .active_tab()
            .file_path
            .as_ref()
            .unwrap()
            .file_name()
            .unwrap(),
        "main.rs"
    );
    assert!(test_dir.path().join("main.rs").exists());
}

#[test]
fn test_app_explorer_rename_file_via_prompt_updates_open_tab() {
    let test_dir = TestDir::new("lume_test_app_rename_flow");
    let file_path = test_dir.path().join("alpha.txt");
    std::fs::write(&file_path, "contents").unwrap();

    let mut app = App::new(Some(test_dir.path().to_path_buf())).unwrap();
    app.editor.open_or_load(&file_path).unwrap();

    app.focus = ActiveFocus::Explorer;
    app.explorer.refresh();
    app.explorer.selected_idx = 0;

    // Press 'm' to open rename prompt
    app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE))
        .unwrap();
    assert!(app.prompt.is_some());
    assert_eq!(app.prompt.as_ref().unwrap().value, "alpha.txt");

    // Clear and type "beta.txt"
    for _ in 0.."alpha.txt".len() {
        app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
            .unwrap();
    }
    for c in "beta.txt".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();

    assert!(app.prompt.is_none());
    assert!(!file_path.exists());
    assert!(test_dir.path().join("beta.txt").exists());
    assert_eq!(
        app.editor
            .active_tab()
            .file_path
            .as_ref()
            .unwrap()
            .file_name()
            .unwrap(),
        "beta.txt"
    );
}

#[test]
fn test_app_explorer_single_letter_jump_navigation() {
    let test_dir = TestDir::new("lume_test_app_letter_jump");
    std::fs::write(test_dir.path().join("apple.rs"), "").unwrap();
    std::fs::write(test_dir.path().join("banana.rs"), "").unwrap();
    std::fs::write(test_dir.path().join("cherry.rs"), "").unwrap();

    let mut app = App::new(Some(test_dir.path().to_path_buf())).unwrap();
    app.focus = ActiveFocus::Explorer;
    app.explorer.refresh();
    assert_eq!(
        app.explorer.items[app.explorer.selected_idx].name,
        "apple.rs"
    );

    // Press 'b' in explorer -> jumps to banana.rs
    app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(
        app.explorer.items[app.explorer.selected_idx].name,
        "banana.rs"
    );

    // Press 'c' in explorer -> jumps to cherry.rs
    app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(
        app.explorer.items[app.explorer.selected_idx].name,
        "cherry.rs"
    );
}

#[test]
fn test_app_explorer_f2_rename_routing_and_empty_rename() {
    let test_dir = TestDir::new("lume_test_f2_rename");
    std::fs::write(test_dir.path().join("file.txt"), "data").unwrap();

    let mut app = App::new(Some(test_dir.path().to_path_buf())).unwrap();
    app.focus = ActiveFocus::Explorer;
    app.explorer.refresh();
    assert_eq!(app.explorer.items.len(), 1);

    // F2 while in explorer should trigger rename
    app.handle_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE))
        .unwrap();
    assert!(app.prompt.is_some());
    match app.prompt.as_ref().unwrap().action {
        PromptAction::RenameItem {
            ref source_path, ..
        } => {
            assert_eq!(source_path.file_name().unwrap(), "file.txt");
        }
        _ => panic!("Expected RenameItem"),
    }
    app.prompt = None;

    // When explorer has no items, rename command gives status message
    app.explorer.items.clear();
    app.execute_command("explorer.rename");
    assert_eq!(
        app.status_message.as_ref().unwrap().0,
        "No item selected to rename"
    );

    // Root cannot be renamed
    app.explorer.items.push(crate::explorer::ExplorerItem {
        path: app.explorer.root.clone(),
        name: "root".to_string(),
        is_dir: true,
        depth: 0,
        is_expanded: true,
    });
    app.explorer.selected_idx = 0;
    app.execute_command("explorer.rename");
    assert_eq!(
        app.status_message.as_ref().unwrap().0,
        "Cannot rename workspace root directory"
    );
}

#[test]
fn test_app_explorer_create_file_and_rename_error_prompt_handling() {
    let test_dir = TestDir::new("lume_test_err_prompts");
    let mut app = App::new(Some(test_dir.path().to_path_buf())).unwrap();

    // Create file error handling (invalid path traversal)
    app.prompt = Some(InputPrompt::create_file(
        test_dir.path().to_path_buf(),
        "workspace",
    ));
    for c in "/root/secret.txt".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert!(app
        .status_message
        .as_ref()
        .unwrap()
        .0
        .contains("Create file error"));

    // Rename error handling (invalid name)
    let f1 = test_dir.path().join("doc.txt");
    std::fs::write(&f1, "txt").unwrap();
    app.prompt = Some(InputPrompt::rename_item(f1, "doc.txt", false));
    for _ in 0.."doc.txt".len() {
        app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
            .unwrap();
    }
    for c in "/forbidden/name.txt".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert!(app
        .status_message
        .as_ref()
        .unwrap()
        .0
        .contains("Rename error"));
}

#[test]
fn test_app_rename_and_delete_tabs_with_symlink_ancestor_resolution() {
    let test_dir = TestDir::new("lume_test_symlink_res");
    let ws = test_dir.path();
    let folder = ws.join("real_folder");
    std::fs::create_dir_all(&folder).unwrap();

    let file_in_folder = folder.join("file.rs");
    std::fs::write(&file_in_folder, "fn test() {}").unwrap();

    let mut app = App::new(Some(ws.to_path_buf())).unwrap();
    app.editor.open_or_load(&file_in_folder).unwrap();

    // Renaming the real folder must update open tab
    let renamed_folder = ws.join("renamed_folder");
    std::fs::rename(&folder, &renamed_folder).unwrap();
    app.handle_renamed_path_tabs(&folder, &renamed_folder);

    assert_eq!(
        app.editor
            .active_tab()
            .file_path
            .as_ref()
            .unwrap()
            .file_name()
            .unwrap(),
        "file.rs"
    );
    assert!(app
        .editor
        .active_tab()
        .file_path
        .as_ref()
        .unwrap()
        .starts_with(&renamed_folder));

    // Deleting the renamed folder must unlink tab
    std::fs::remove_dir_all(&renamed_folder).unwrap();
    app.handle_deleted_folder_tabs(&renamed_folder);
    assert!(app.editor.active_tab().file_path.is_none());

    // Direct rename test with ancestor mismatch
    let old_dir = ws.join("ancestor_old");
    let nested_dir = old_dir.join("sub");
    std::fs::create_dir_all(&nested_dir).unwrap();
    let nested_file = nested_dir.join("test.txt");
    std::fs::write(&nested_file, "content").unwrap();

    let mut app2 = App::new(Some(ws.to_path_buf())).unwrap();
    app2.editor.open_or_load(&nested_file).unwrap();

    let new_dir = ws.join("ancestor_new");
    std::fs::rename(&old_dir, &new_dir).unwrap();
    app2.handle_renamed_path_tabs(&old_dir, &new_dir);

    let updated_path = app2.editor.active_tab().file_path.as_ref().unwrap();
    assert!(
        updated_path.starts_with(&new_dir)
            || updated_path.ends_with(std::path::Path::new("ancestor_new/sub/test.txt"))
    );
}
