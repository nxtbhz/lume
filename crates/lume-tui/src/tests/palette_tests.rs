use crate::app::{ActiveFocus, App};
use crate::vim::VimMode;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use lume_core::TextBuffer;

#[test]
fn test_app_creation_and_command_palette() {
    let mut app = App::new(None).unwrap();
    assert_eq!(app.editor.tabs().len(), 1);
    assert!(!app.palette_open);

    // Open palette
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL))
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

    // Open palette with Ctrl+P
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL))
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
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL))
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
