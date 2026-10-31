use crate::app::App;
use crate::tests::TestDir;
use crate::vim::VimMode;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use lume_core::TextBuffer;

#[test]
fn test_app_vim_modal_toggle_and_commands() {
    let mut app = App::new(None).unwrap();
    assert!(!app.modal_enabled);

    // Test :set modal command idempotently enables
    app.execute_command(":set modal");
    assert!(app.modal_enabled);
    assert_eq!(app.vim_state.mode, VimMode::Normal);
    app.execute_command(":set modal");
    assert!(app.modal_enabled);

    // Test :set nomodal command idempotently disables
    app.execute_command(":set nomodal");
    assert!(!app.modal_enabled);
    app.execute_command(":set nomodal");
    assert!(!app.modal_enabled);

    // Test Ctrl+Alt+V shortcut toggle
    let key_v = KeyEvent::new(
        KeyCode::Char('v'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    );
    app.handle_key(key_v).unwrap();
    assert!(app.modal_enabled);
    assert_eq!(app.vim_state.mode, VimMode::Normal);

    // Toggle back off with Ctrl+Alt+V
    app.handle_key(key_v).unwrap();
    assert!(!app.modal_enabled);

    // Test :set modal! command
    app.execute_command(":set modal!");
    assert!(app.modal_enabled);
    app.execute_command(":set modal!");
    assert!(!app.modal_enabled);
}

#[test]
fn test_app_vim_motions() {
    let mut app = App::new(None).unwrap();
    app.execute_command(":set modal");
    assert!(app.modal_enabled);
    assert_eq!(app.vim_state.mode, VimMode::Normal);

    // 1. Enter insert mode with 'i', type text, and return to Normal with Esc
    let key_i = KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE);
    app.handle_key(key_i).unwrap();
    assert_eq!(app.vim_state.mode, VimMode::Insert);

    for c in "hello world\nline two\n".chars() {
        if c == '\n' {
            app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
                .unwrap();
        } else {
            app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
                .unwrap();
        }
    }
    assert_eq!(
        app.editor.buffer().rope().to_string(),
        "hello world\nline two\n"
    );

    let key_esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    app.handle_key(key_esc).unwrap();
    assert_eq!(app.vim_state.mode, VimMode::Normal);

    // 2. Navigation: gg, G, h, j, k, l, w, b, 0, $
    let key_g = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
    app.handle_key(key_g).unwrap();
    assert_eq!(app.vim_state.pending_operator, Some('g'));
    app.handle_key(key_g).unwrap(); // gg -> start
    assert_eq!(app.editor.cursors().primary().head, 0);
    assert_eq!(app.vim_state.pending_operator, None);

    let key_cap_g = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT);
    app.handle_key(key_cap_g).unwrap(); // G -> end
    assert_eq!(
        app.editor.cursors().primary().head,
        app.editor.buffer().len_bytes()
    );

    app.handle_key(key_g).unwrap();
    app.handle_key(key_g).unwrap(); // gg -> start (0)

    let key_w = KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE);
    app.handle_key(key_w).unwrap();
    assert_eq!(app.editor.cursors().primary().head, 6); // start of "world"

    let key_b = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE);
    app.handle_key(key_b).unwrap();
    assert_eq!(app.editor.cursors().primary().head, 0); // start of "hello"

    let key_dollar = KeyEvent::new(KeyCode::Char('$'), KeyModifiers::NONE);
    app.handle_key(key_dollar).unwrap();
    assert_eq!(app.editor.cursors().primary().head, 11); // end of line 1

    let key_zero = KeyEvent::new(KeyCode::Char('0'), KeyModifiers::NONE);
    app.handle_key(key_zero).unwrap();
    assert_eq!(app.editor.cursors().primary().head, 0); // start of line 1

    let key_j = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
    app.handle_key(key_j).unwrap(); // down to line 2
    let key_k = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
    app.handle_key(key_k).unwrap(); // up to line 1
    assert_eq!(app.editor.cursors().primary().head, 0);

    let key_l = KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE);
    app.handle_key(key_l).unwrap(); // right 1
    assert_eq!(app.editor.cursors().primary().head, 1);
    let key_h = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE);
    app.handle_key(key_h).unwrap(); // left 1
    assert_eq!(app.editor.cursors().primary().head, 0);

    // 3. Editing: x, dd, yy, p, u, Ctrl+R
    let key_x = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
    app.handle_key(key_x).unwrap(); // delete 'h'
    assert_eq!(
        app.editor.buffer().rope().to_string(),
        "ello world\nline two\n"
    );

    let key_u = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE);
    app.handle_key(key_u).unwrap(); // undo
    assert_eq!(
        app.editor.buffer().rope().to_string(),
        "hello world\nline two\n"
    );

    let key_ctrl_r = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL);
    app.handle_key(key_ctrl_r).unwrap(); // redo
    assert_eq!(
        app.editor.buffer().rope().to_string(),
        "ello world\nline two\n"
    );

    app.handle_key(key_u).unwrap(); // undo back to original
    assert_eq!(
        app.editor.buffer().rope().to_string(),
        "hello world\nline two\n"
    );

    // yy (yank line)
    let key_y = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
    app.handle_key(key_y).unwrap();
    assert_eq!(app.vim_state.pending_operator, Some('y'));
    app.handle_key(key_y).unwrap();
    assert_eq!(app.vim_state.pending_operator, None);
    assert_eq!(
        app.vim_state.yank_register,
        Some(("hello world\n".to_string(), true))
    );

    // dd (delete line)
    let key_d = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE);
    app.handle_key(key_d).unwrap();
    assert_eq!(app.vim_state.pending_operator, Some('d'));
    app.handle_key(key_d).unwrap();
    assert_eq!(app.vim_state.pending_operator, None);
    assert_eq!(app.editor.buffer().rope().to_string(), "line two\n");

    // p (paste line below)
    let key_p = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE);
    app.handle_key(key_p).unwrap();
    assert_eq!(
        app.editor.buffer().rope().to_string(),
        "line two\nhello world\n"
    );

    // 4. Mode transitions: a, o, O
    app.handle_key(key_g).unwrap();
    app.handle_key(key_g).unwrap(); // gg -> top
    let key_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    app.handle_key(key_a).unwrap();
    assert_eq!(app.vim_state.mode, VimMode::Insert);
    assert_eq!(app.editor.cursors().primary().head, 1);
    app.handle_key(key_esc).unwrap();

    let key_o = KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE);
    app.handle_key(key_o).unwrap();
    assert_eq!(app.vim_state.mode, VimMode::Insert);
    app.handle_key(key_esc).unwrap();

    let key_cap_o = KeyEvent::new(KeyCode::Char('O'), KeyModifiers::SHIFT);
    app.handle_key(key_cap_o).unwrap();
    assert_eq!(app.vim_state.mode, VimMode::Insert);
    app.handle_key(key_esc).unwrap();

    // 5. Visual mode: v, selection, y, d
    let key_v = KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE);
    app.handle_key(key_v).unwrap();
    assert_eq!(app.vim_state.mode, VimMode::Visual);

    app.handle_key(key_l).unwrap();
    app.handle_key(key_l).unwrap();
    assert!(app.editor.cursors().primary().is_selection());

    // y in visual mode yanks and returns to Normal mode
    app.handle_key(key_y).unwrap();
    assert_eq!(app.vim_state.mode, VimMode::Normal);
    assert!(!app.editor.cursors().primary().is_selection());
    assert!(app.vim_state.yank_register.is_some());

    // Visual mode: delete selection with d/x
    app.handle_key(key_v).unwrap();
    assert_eq!(app.vim_state.mode, VimMode::Visual);
    app.handle_key(key_w).unwrap();
    assert!(app.editor.cursors().primary().is_selection());
    app.handle_key(key_d).unwrap(); // delete selection
    assert_eq!(app.vim_state.mode, VimMode::Normal);
    assert!(!app.editor.cursors().primary().is_selection());

    // Visual mode: cancel with Esc
    app.handle_key(key_v).unwrap();
    assert_eq!(app.vim_state.mode, VimMode::Visual);
    app.handle_key(key_esc).unwrap();
    assert_eq!(app.vim_state.mode, VimMode::Normal);

    // Visual mode: cancel with v
    app.handle_key(key_v).unwrap();
    assert_eq!(app.vim_state.mode, VimMode::Visual);
    app.handle_key(key_v).unwrap();
    assert_eq!(app.vim_state.mode, VimMode::Normal);

    // Normal mode: PageUp / PageDown
    app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE))
        .unwrap();

    // Pending operator cancel on non-matching key
    app.handle_key(key_g).unwrap();
    assert_eq!(app.vim_state.pending_operator, Some('g'));
    app.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.vim_state.pending_operator, None);

    // Esc clears pending operator in Normal mode
    app.handle_key(key_d).unwrap();
    assert_eq!(app.vim_state.pending_operator, Some('d'));
    app.handle_key(key_esc).unwrap();
    assert_eq!(app.vim_state.pending_operator, None);

    // Visual mode: 'g' pending operator cleared by non-'g' key
    app.handle_key(key_v).unwrap();
    assert_eq!(app.vim_state.mode, VimMode::Visual);
    app.handle_key(key_g).unwrap();
    assert_eq!(app.vim_state.pending_operator, Some('g'));
    app.handle_key(key_w).unwrap(); // another key clears pending 'g'
    assert_eq!(app.vim_state.pending_operator, None);
    app.handle_key(key_esc).unwrap();

    // Test xp character transposition in Normal mode
    let mut app_xp = App::new(None).unwrap();
    app_xp.execute_command(":set modal");
    app_xp.handle_key(key_i).unwrap();
    for c in "ab".chars() {
        app_xp
            .handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }
    app_xp.handle_key(key_esc).unwrap();
    app_xp.handle_key(key_g).unwrap();
    app_xp.handle_key(key_g).unwrap(); // cursor at 0 ('a')
    app_xp.handle_key(key_x).unwrap(); // cuts 'a'
    assert_eq!(
        app_xp.vim_state.yank_register,
        Some(("a".to_string(), false))
    );
    app_xp.handle_key(key_p).unwrap(); // pastes 'a' after 'b' -> "ba"
    assert_eq!(app_xp.editor.buffer().rope().to_string(), "ba");
}

#[test]
fn test_app_vim_status_bar_rendering() {
    let mut app = App::new(None).unwrap();
    app.terminal_width = 120;
    app.terminal_height = 24;

    // 1. Modeless (default): no vim indicators in rendered buffer
    let mut buf = Vec::new();
    app.render(&mut buf).unwrap();
    let rendered_default = String::from_utf8_lossy(&buf);
    assert!(!rendered_default.contains("[NORMAL]"));
    assert!(!rendered_default.contains("[INSERT]"));
    assert!(!rendered_default.contains("[VISUAL]"));

    // 2. Normal mode: contains [NORMAL]
    app.execute_command(":set modal");
    buf.clear();
    app.render(&mut buf).unwrap();
    let rendered_normal = String::from_utf8_lossy(&buf);
    assert!(rendered_normal.contains("[NORMAL]"));

    // 3. Insert mode: contains [INSERT]
    app.vim_state.set_mode(VimMode::Insert);
    buf.clear();
    app.render(&mut buf).unwrap();
    let rendered_insert = String::from_utf8_lossy(&buf);
    assert!(rendered_insert.contains("[INSERT]"));

    // 4. Visual mode: contains [VISUAL]
    app.vim_state.set_mode(VimMode::Visual);
    buf.clear();
    app.render(&mut buf).unwrap();
    let rendered_visual = String::from_utf8_lossy(&buf);
    assert!(rendered_visual.contains("[VISUAL]"));

    // 5. Toggle off: back to no vim indicators
    app.execute_command(":set nomodal");
    buf.clear();
    app.render(&mut buf).unwrap();
    let rendered_off = String::from_utf8_lossy(&buf);
    assert!(!rendered_off.contains("[NORMAL]"));
    assert!(!rendered_off.contains("[INSERT]"));
    assert!(!rendered_off.contains("[VISUAL]"));
}

#[test]
fn test_app_vim_insert_and_visual_full_motions() {
    let mut app = App::new(None).unwrap();
    // Toggle modal via Ctrl+Alt+V
    app.handle_key(KeyEvent::new(
        KeyCode::Char('v'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    ))
    .unwrap();
    assert!(app.modal_enabled);
    assert_eq!(app.vim_state.mode, VimMode::Normal);

    // Enter Insert mode and exercise all navigation/edit keys
    app.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.vim_state.mode, VimMode::Normal);

    // Normal mode PageUp/PageDown and 'x' with selection
    app.editor
        .insert_str("first line\nsecond line\nthird line")
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE))
        .unwrap();
    app.editor.select_word_at_cursor();
    app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))
        .unwrap();

    // Visual mode full motions: j, k, h, l, b, 0, $, G, gg, y
    app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.vim_state.mode, VimMode::Visual);
    for ch in ['j', 'k', 'l', 'h', 'w', 'b', '0', '$', 'G'] {
        app.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE))
            .unwrap();
    }
    // Visual 'gg'
    app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.editor.cursors().primary().head, 0);
}

#[test]
fn test_goto_line_key_routing_and_vim_colon() {
    let test_dir = TestDir::new("lume_test_goto_keys");
    let mut app = App::new(Some(test_dir.path().to_path_buf())).unwrap();

    // 1. Modeless Ctrl+G
    assert!(app.prompt.is_none());
    app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.prompt.is_some());
    assert_eq!(
        app.prompt.as_ref().unwrap().action,
        crate::prompt::PromptAction::GoToLine
    );
    app.prompt = None;

    // 2. Modeless Ctrl+Shift+G / CapsLock G
    app.handle_key(KeyEvent::new(
        KeyCode::Char('G'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ))
    .unwrap();
    assert!(app.prompt.is_some());
    assert_eq!(
        app.prompt.as_ref().unwrap().action,
        crate::prompt::PromptAction::GoToLine
    );
    app.prompt = None;

    // 3. Vim Normal mode `:`
    app.execute_command(":set modal");
    assert!(app.modal_enabled);
    app.handle_key(KeyEvent::new(KeyCode::Char(':'), KeyModifiers::NONE))
        .unwrap();
    assert!(app.prompt.is_some());
    assert_eq!(
        app.prompt.as_ref().unwrap().action,
        crate::prompt::PromptAction::GoToLine
    );
}
