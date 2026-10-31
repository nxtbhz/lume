use std::path::PathBuf;

use crate::app::{ActiveFocus, App};
use crate::commands::COMMANDS;
use crate::prompt::{InputPrompt, PromptAction};
use crate::tests::TestDir;
use crate::vim::VimMode;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use lume_core::TextBuffer;

#[test]
fn test_app_creation_and_command_palette() {
    let mut app = App::new(None).unwrap();
    assert_eq!(app.editor.tabs().len(), 1);
    assert!(!app.palette_open);

    // Open palette with Ctrl+Shift+P
    app.handle_key(KeyEvent::new(KeyCode::Char('P'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.palette_open);

    // Type query 'save'
    app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE))
        .unwrap();

    let filtered = app.filtered_commands();
    assert!(!filtered.is_empty());
    assert!(filtered.iter().any(|c| c.id == "file.save"));

    // Esc closes palette
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .unwrap();
    assert!(!app.palette_open);
}

#[test]
fn test_app_which_key_toggle() {
    let mut app = App::new(None).unwrap();
    assert!(!app.which_key_open);

    // F1 opens which-key
    app.handle_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE))
        .unwrap();
    assert!(app.which_key_open);

    // Any key dismisses it
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .unwrap();
    assert!(!app.which_key_open);
}

#[test]
fn test_app_palette_lifecycle_and_rendering() {
    let mut app = App::new(None).unwrap();

    // Open palette with Ctrl+Shift+P
    app.handle_key(KeyEvent::new(KeyCode::Char('P'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.palette_open);

    // Filter commands
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE))
        .unwrap();
    assert!(!app.filtered_commands().is_empty());

    // Navigate up & down
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
        .unwrap();

    // Backspace on query
    app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
        .unwrap();

    // Render full frame with palette open
    let mut buf = Vec::new();
    assert!(app.render(&mut buf).is_ok());
    assert!(!buf.is_empty());

    // Close palette with Esc
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .unwrap();
    assert!(!app.palette_open);

    // Open palette and execute command via Enter
    app.handle_key(KeyEvent::new(KeyCode::Char('P'), KeyModifiers::CONTROL))
        .unwrap();
    app.palette_query = "New Tab".to_string();
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert!(!app.palette_open);
    assert_eq!(app.editor.tabs().len(), 2);
}

#[test]
fn test_app_execute_all_commands() {
    let mut app = App::new(None).unwrap();

    // File & Tabs
    app.execute_command("file.new_tab");
    assert_eq!(app.editor.tabs().len(), 2);
    app.execute_command("file.prev_tab");
    assert_eq!(app.editor.active_tab_idx(), 0);
    app.execute_command("file.next_tab");
    assert_eq!(app.editor.active_tab_idx(), 1);
    app.execute_command("file.close_tab");
    assert_eq!(app.editor.tabs().len(), 1);

    // Explorer commands
    assert!(app.explorer.is_visible);
    app.execute_command("view.toggle_explorer");
    assert!(!app.explorer.is_visible);
    assert_eq!(app.focus, ActiveFocus::Editor);
    app.execute_command("view.toggle_explorer");
    assert!(app.explorer.is_visible);
    assert_eq!(app.focus, ActiveFocus::Explorer);
    app.execute_command("explorer.refresh");
    app.execute_command("explorer.new_folder");
    assert!(app.prompt.is_some());
    app.prompt = None;
    app.execute_command("explorer.delete_folder");

    // Prompts
    app.execute_command("file.open");
    assert!(app.prompt.is_some());
    app.prompt = None;

    app.execute_command("file.save");
    assert!(app.prompt.is_some()); // untitled file triggers save_as prompt
    app.prompt = None;

    app.execute_command("file.save_as");
    assert!(app.prompt.is_some());
    app.prompt = None;

    app.execute_command("language.set_mode");
    assert!(app.prompt.is_some());
    app.prompt = None;

    // Undo & Redo
    app.editor.insert_str("test").unwrap();
    app.execute_command("edit.undo");
    assert_eq!(app.editor.buffer().len_bytes(), 0);
    app.execute_command("edit.redo");
    assert_eq!(app.editor.buffer().len_bytes(), 4);

    // Find & Replace
    app.execute_command("edit.find");
    assert!(app.search_state.is_some());
    app.search_state = None;

    app.execute_command("edit.replace");
    assert!(app.search_state.is_some());
    app.search_state = None;

    // Which-Key & Line Numbers
    assert!(!app.which_key_open);
    app.execute_command("view.which_key");
    assert!(app.which_key_open);
    app.which_key_open = false;

    assert!(app.show_line_numbers);
    app.execute_command("view.toggle_line_numbers");
    assert!(!app.show_line_numbers);
    app.execute_command("view.toggle_line_numbers");
    assert!(app.show_line_numbers);

    // Modal toggle
    assert!(!app.modal_enabled);
    app.execute_command("settings.toggle_modal");
    assert!(app.modal_enabled);
    assert_eq!(app.vim_state.mode, VimMode::Normal);
    app.execute_command(":set nomodal");
    assert!(!app.modal_enabled);
    app.execute_command(":set modal");
    assert!(app.modal_enabled);
}

#[test]
fn test_app_page_up_and_page_down() {
    let mut app = App::new(None).unwrap();
    let content = (1..=100)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n");
    app.editor.insert_str(&content).unwrap();
    app.editor.move_lines(-100, false);
    app.adjust_viewport();

    assert_eq!(app.editor.cursor_line_col().0, 0);

    // Page Down
    app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE))
        .unwrap();
    let h = app.text_area_height();
    assert_eq!(app.editor.cursor_line_col().0, h);

    // Page Up
    app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.editor.cursor_line_col().0, 0);
}

#[test]
fn test_app_events_tick_and_overlay_edge_cases() {
    use crate::input::AppEvent;
    use crate::prompt::InputPrompt;
    use crate::tests::TestDir;

    let test_dir = TestDir::new("lume_events_test");
    let file_path = test_dir.path().join("saved.rs");
    std::fs::write(&file_path, "fn hello() {}\n").unwrap();

    // Test App::new with directory and file
    let app_dir = App::new(Some(test_dir.path().to_path_buf())).unwrap();
    assert!(app_dir.editor.file_path().is_none());

    let mut app = App::new(Some(file_path.clone())).unwrap();
    let mut sink = std::io::sink();

    // AppEvent::Resize
    app.handle_event(AppEvent::Resize(100, 30), &mut sink)
        .unwrap();
    assert_eq!((app.terminal_width, app.terminal_height), (100, 30));

    // AppEvent::Input
    app.handle_event(
        AppEvent::Input(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)),
        &mut sink,
    )
    .unwrap();
    assert!(app.editor.is_dirty());

    // AppEvent::FocusLost triggers async save when dirty and file_path is set
    app.handle_event(AppEvent::FocusLost, &mut sink).unwrap();
    for _ in 0..20 {
        app.handle_tick(&mut sink).unwrap();
        if !app.editor.is_dirty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    // Selection-seeded edit.find and edit.replace
    app.editor.cursors_mut().set_single(0);
    app.editor.move_right(true);
    app.editor.move_right(true);
    app.execute_command("edit.find");
    assert!(app.search_state.is_some());
    // Shift+Enter, Backspace, Ctrl+Y inside search overlay
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .unwrap();

    app.editor.cursors_mut().set_single(0);
    app.editor.move_right(true);
    app.execute_command("edit.replace");
    assert!(app.search_state.is_some());
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .unwrap();

    // Prompt Esc, Plain Text language, Unknown language, Open non-existent file, Cancel folder delete
    app.execute_command("file.open");
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .unwrap();
    assert!(app.prompt.is_none());

    app.execute_command("file.open");
    for c in "does_not_exist_12345.rs".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();

    app.prompt = Some(InputPrompt::set_language(&[]));
    for c in "Plain Text".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();

    app.prompt = Some(InputPrompt::set_language(&[]));
    for c in "InvalidLangXYZ".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();

    app.prompt = Some(InputPrompt::confirm_delete_folder(
        test_dir.path().to_path_buf(),
    ));
    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();

    app.execute_command("app.quit");
    assert!(!app.running);
}

#[test]
fn test_prompt_create_file_and_rename_item_constructors() {
    let p1 = InputPrompt::create_file(PathBuf::from("/workspace/src"), "src");
    assert_eq!(
        p1.action,
        PromptAction::CreateFile {
            parent_dir: PathBuf::from("/workspace/src")
        }
    );
    assert!(p1.title.contains("Create File in 'src'"));
    assert_eq!(p1.value, "");

    let p2 = InputPrompt::rename_item(PathBuf::from("/workspace/src/lib.rs"), "lib.rs", false);
    assert_eq!(
        p2.action,
        PromptAction::RenameItem {
            source_path: PathBuf::from("/workspace/src/lib.rs"),
            is_dir: false
        }
    );
    assert!(p2.title.contains("Rename file 'lib.rs'"));
    assert_eq!(p2.value, "lib.rs");
    assert_eq!(p2.cursor_pos, "lib.rs".len());
}

#[test]
fn test_commands_explorer_new_file_and_rename_execution() {
    let test_dir = TestDir::new("lume_test_cmd_explorer");
    let mut app = App::new(Some(test_dir.path().to_path_buf())).unwrap();

    app.execute_command("explorer.new_file");
    assert!(app.prompt.is_some());
    match app.prompt.as_ref().unwrap().action {
        PromptAction::CreateFile { .. } => {}
        _ => panic!("Expected PromptAction::CreateFile"),
    }

    // When item is selected, rename should open rename prompt with current name
    std::fs::write(test_dir.path().join("test.txt"), "").unwrap();
    app.explorer.refresh();
    app.explorer.selected_idx = 0;

    app.execute_command("explorer.rename");
    assert!(app.prompt.is_some());
    match app.prompt.as_ref().unwrap().action {
        PromptAction::RenameItem {
            ref source_path,
            is_dir,
        } => {
            assert_eq!(source_path.file_name().unwrap(), "test.txt");
            assert!(!is_dir);
        }
        _ => panic!("Expected PromptAction::RenameItem"),
    }
}

#[test]
fn test_commands_navigation_goto_line_execution() {
    let test_dir = TestDir::new("lume_test_goto_cmd");
    let mut app = App::new(Some(test_dir.path().to_path_buf())).unwrap();

    // Verify command exists in catalog
    let cmd = COMMANDS.iter().find(|c| c.id == "navigation.goto_line");
    assert!(cmd.is_some(), "navigation.goto_line must be registered in COMMANDS");
    let cmd = cmd.unwrap();
    assert_eq!(cmd.shortcut, "Ctrl+G");

    // Execute command
    assert!(app.prompt.is_none());
    app.execute_command("navigation.goto_line");
    assert!(app.prompt.is_some());
    let prompt = app.prompt.as_ref().unwrap();
    assert_eq!(prompt.action, crate::prompt::PromptAction::GoToLine);
}
