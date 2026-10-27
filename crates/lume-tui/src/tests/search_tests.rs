use std::path::Path;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::App;
use crate::prompt::InputPrompt;
use crate::search::SearchMode;
use crate::tests::TestDir;
use lume_core::TextBuffer;
use lume_syntax::{Language, TokenType};

#[test]
fn test_app_find_incremental_and_navigation() {
    let mut app = App::new(None).unwrap();
    app.editor
        .insert_str("alpha beta alpha gamma alpha")
        .unwrap();
    app.editor.cursors_mut().set_single(0);

    // 1. Trigger Ctrl+F
    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.search_state.is_some());

    // 2. Type "alpha"
    for c in "alpha".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }
    assert_eq!(app.editor.cursors().primary().range(), 0..5);

    // 3. Advance to next match via Enter
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.editor.cursors().primary().range(), 11..16);

    // 4. Advance via F3
    app.handle_key(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.editor.cursors().primary().range(), 23..28);

    // 5. Previous via Shift+F3
    app.handle_key(KeyEvent::new(KeyCode::F(3), KeyModifiers::SHIFT))
        .unwrap();
    assert_eq!(app.editor.cursors().primary().range(), 11..16);

    // 6. Esc cancels and restores initial position (0)
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .unwrap();
    assert!(app.search_state.is_none());
    assert_eq!(app.editor.cursors().primary().head, 0);
}

#[test]
fn test_app_replace_current_and_replace_all_atomic() {
    let mut app = App::new(None).unwrap();
    app.editor.insert_str("val other val extra val").unwrap();
    app.editor.cursors_mut().set_single(0);

    // Trigger Ctrl+H
    app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.search_state.is_some());

    // Type query "val"
    for c in "val".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }
    // Switch to Replacement field
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .unwrap();
    for c in "result".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }

    // Replace current occurrence via Enter
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert!(app
        .editor
        .buffer()
        .slice_to_cow(0, app.editor.buffer().len_bytes())
        .starts_with("result other"));

    // Replace all remaining via Ctrl+Alt+Enter
    let ctrl_alt_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL | KeyModifiers::ALT);
    app.handle_key(ctrl_alt_enter).unwrap();
    assert_eq!(
        app.editor
            .buffer()
            .slice_to_cow(0, app.editor.buffer().len_bytes()),
        "result other result extra result"
    );

    // First undo via Ctrl+Z while search bar is still open reverts the atomic replace_all
    app.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(
        app.editor
            .buffer()
            .slice_to_cow(0, app.editor.buffer().len_bytes()),
        "result other val extra val"
    );

    // Second undo via Ctrl+Z reverts the earlier single replace_current
    app.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(
        app.editor
            .buffer()
            .slice_to_cow(0, app.editor.buffer().len_bytes()),
        "val other val extra val"
    );
}

#[test]
fn test_app_search_esc_clamps_cursor_when_file_shortened() {
    let mut app = App::new(None).unwrap();
    app.editor
        .insert_str("a long text that will be drastically shortened")
        .unwrap();
    let len = app.editor.buffer().len_bytes();
    app.editor.cursors_mut().set_single(len);

    // Open Ctrl+H
    app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL))
        .unwrap();

    // Shorten buffer
    let _ = app.editor.replace_all(
        "a long text that will be drastically shortened",
        "short",
        false,
    );
    assert_eq!(app.editor.buffer().len_bytes(), 5);

    // Press Esc: must not panic on restoring pre-search cursor (> 5)
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .unwrap();
    assert!(app.search_state.is_none());
    assert_eq!(app.editor.cursors().primary().head, 5);
}

#[test]
fn test_app_f3_on_replacement_field_navigates_without_replacing() {
    let mut app = App::new(None).unwrap();
    app.editor.insert_str("cat dog cat bird cat").unwrap();
    app.editor.cursors_mut().set_single(0);

    // Open Replace mode
    app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL))
        .unwrap();
    // Type query "cat"
    for c in "cat".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }
    // Switch to Replacement field
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .unwrap();
    for c in "lion".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }

    // Press F3: should advance search to match 2, NOT replace!
    app.handle_key(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(
        app.editor
            .buffer()
            .slice_to_cow(0, app.editor.buffer().len_bytes()),
        "cat dog cat bird cat"
    );
    assert_eq!(app.editor.cursors().primary().range(), 8..11);
}

#[test]
fn test_app_active_match_idx_sync() {
    let mut app = App::new(None).unwrap();
    app.editor
        .insert_str("zero one match one match two match three match")
        .unwrap();
    // Place initial cursor before match #2 (at offset 18)
    app.editor.cursors_mut().set_single(18);

    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL))
        .unwrap();
    for c in "match".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }

    // Active match should be match #2 (index 1), with total 4 matches
    let state = app.search_state.as_ref().unwrap();
    assert_eq!(state.match_count, 4);
    assert_eq!(state.active_match_idx, 1);

    // Advance to match #3
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    let state = app.search_state.as_ref().unwrap();
    assert_eq!(state.active_match_idx, 2);
}

#[test]
fn test_app_file_explorer_toggle() {
    let mut app = App::new(None).unwrap();
    assert!(app.explorer.is_visible);

    // F2 toggles explorer
    app.handle_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE))
        .unwrap();
    assert!(!app.explorer.is_visible);

    app.handle_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE))
        .unwrap();
    assert!(app.explorer.is_visible);
}

#[test]
fn test_app_prompt_save_and_open() {
    let mut app = App::new(None).unwrap();
    app.editor.insert_str("test file content").unwrap();

    // Ctrl+S triggers Save As prompt on untitled buffer
    app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.prompt.is_some());

    // Type target filename
    let target_path = "/tmp/lume_test_save.txt";
    for c in target_path.chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }

    // Enter submits save
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert!(app.prompt.is_none());
    assert_eq!(app.editor.file_path(), Some(Path::new(target_path)));
    assert!(!app.editor.is_dirty());

    // Open File prompt
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.prompt.is_some());

    for c in target_path.chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert!(app.prompt.is_none());

    let _ = std::fs::remove_file(target_path);
}

#[test]
fn test_app_language_override() {
    let mut app = App::new(None).unwrap();
    assert_eq!(app.current_language(), None);

    // Set language mode via command
    app.execute_command("language.set_mode");
    assert!(app.prompt.is_some());

    for c in "Rust".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.current_language(), Some(Language::Rust));
}

#[test]
fn test_app_syntax_worker_async_highlighting() {
    let mut app = App::new(None).unwrap();
    app.editor.insert_str("fn main() { let x = 42; }").unwrap();
    app.editor.set_language_override(Some("Rust".to_string()));

    // First render triggers async syntax request
    let mut sink = std::io::sink();
    app.render(&mut sink).unwrap();

    // Wait for background worker to produce tokens (up to 500ms)
    for _ in 0..50 {
        if app.has_pending_syntax_response() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let tokens = app.update_syntax_highlights();
    assert!(
        !tokens.is_empty(),
        "SyntaxWorker should have generated tokens"
    );

    let has_keyword = tokens.iter().any(|t| t.token_type == TokenType::Keyword);
    let has_function = tokens.iter().any(|t| t.token_type == TokenType::Function);
    assert!(has_keyword, "Expected keyword token (e.g. fn / let)");
    assert!(has_function, "Expected function token (e.g. main)");
}

#[test]
fn test_app_ignores_key_release_events() {
    let mut app = App::new(None).unwrap();
    let press =
        KeyEvent::new_with_kind(KeyCode::Char('a'), KeyModifiers::NONE, KeyEventKind::Press);
    let release = KeyEvent::new_with_kind(
        KeyCode::Char('a'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    );

    app.handle_key(press).unwrap();
    app.handle_key(release).unwrap();

    assert_eq!(
        app.editor
            .buffer()
            .slice_to_cow(0, app.editor.buffer().len_bytes()),
        "a"
    );
}

#[test]
fn test_app_f3_and_alt_tab_navigation() {
    let mut app = App::new(None).unwrap();
    app.editor.insert_str("foo bar foo baz foo").unwrap();
    app.last_search_query = Some("foo".to_string());

    // F3 forward search
    app.handle_key(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE))
        .unwrap();
    assert!(app.editor.cursors().primary().is_selection());

    // Shift+F3 backward search
    app.handle_key(KeyEvent::new(KeyCode::F(3), KeyModifiers::SHIFT))
        .unwrap();

    // Alt+Right and Alt+Left tab switching
    assert_eq!(app.editor.active_tab_idx(), 0);
    app.editor.new_tab();
    assert_eq!(app.editor.tabs().len(), 2);
    assert_eq!(app.editor.active_tab_idx(), 1);
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT))
        .unwrap();
    assert_eq!(app.editor.active_tab_idx(), 0);
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT))
        .unwrap();
    assert_eq!(app.editor.active_tab_idx(), 1);
}

#[test]
fn test_app_prompt_key_routing_and_save_with_path() {
    let mut app = App::new(None).unwrap();

    // 1. Open prompt and test keys: Char, Left, Right, Backspace, Down, Up, Tab
    app.prompt = Some(InputPrompt::set_language(&["Rust", "Python", "C", "Go"]));
    app.handle_key(KeyEvent::new(KeyCode::Char('R'), KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert!(app.prompt.is_none());

    // 2. Save file when file path exists
    let test_dir = TestDir::new("lume_save");
    let file_path = test_dir.path().join("test.txt");
    app.editor.set_file_path(Some(file_path.clone()));
    app.editor.insert_str("saved content").unwrap();
    app.execute_command("file.save");
    assert!(file_path.exists());
    assert_eq!(
        std::fs::read_to_string(&file_path).unwrap(),
        "saved content"
    );
}

#[test]
fn test_app_search_key_routing_and_syntax_viewport_render() {
    let mut app = App::new(None).unwrap();
    app.editor
        .insert_str(
            "fn main() {\n    let count = 42;\n    // comment\n    println!(\"hello\");\n}\n",
        )
        .unwrap();
    app.editor.cursors_mut().set_single(0);

    // Open search state and test key routing
    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.search_state.is_some());

    app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE))
        .unwrap();

    // Switch to replace via Ctrl+H and back via Ctrl+F
    app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(app.search_state.as_ref().unwrap().mode, SearchMode::Replace);
    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(app.search_state.as_ref().unwrap().mode, SearchMode::Find);

    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .unwrap();
    assert!(app.search_state.is_none());

    // Populate syntax cache with diverse tokens and render viewport
    use lume_syntax::HighlightToken;
    let tokens = vec![
        HighlightToken {
            start_byte: 0,
            end_byte: 2,
            token_type: TokenType::Keyword,
        },
        HighlightToken {
            start_byte: 3,
            end_byte: 7,
            token_type: TokenType::Function,
        },
        HighlightToken {
            start_byte: 16,
            end_byte: 21,
            token_type: TokenType::Type,
        },
        HighlightToken {
            start_byte: 24,
            end_byte: 26,
            token_type: TokenType::Number,
        },
        HighlightToken {
            start_byte: 31,
            end_byte: 41,
            token_type: TokenType::Comment,
        },
        HighlightToken {
            start_byte: 46,
            end_byte: 54,
            token_type: TokenType::Macro,
        },
        HighlightToken {
            start_byte: 55,
            end_byte: 62,
            token_type: TokenType::String,
        },
    ];
    let buf_id = app.editor.active_tab().id;
    let rev = app.editor.active_tab().revision;
    app.syntax_cache
        .insert(buf_id, (rev, std::sync::Arc::new(tokens)));

    let mut out = Vec::new();
    assert!(app.render(&mut out).is_ok());
    assert!(!out.is_empty());
}
