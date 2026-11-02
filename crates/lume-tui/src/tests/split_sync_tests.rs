use crate::app::App;

#[test]
fn test_close_tab_safely_remaps_split_panes() {
    let mut app = App::new(None).unwrap();
    // Open 3 tabs
    let _ = app.editor.new_tab();
    let _ = app.editor.new_tab();
    assert_eq!(app.editor.tabs().len(), 3);

    // Split view: pane 0 has tab 1, pane 1 has tab 2
    app.splits
        .split_vertical(1, (0, 0), app.editor.cursors().clone());
    app.splits.panes[0].tab_idx = 1;
    app.splits.panes[1].tab_idx = 2;

    // Close Tab 0 -> indices 1 and 2 shift down to 0 and 1
    app.close_tab_and_sync_splits(0);
    assert_eq!(app.splits.panes[0].tab_idx, 0);
    assert_eq!(app.splits.panes[1].tab_idx, 1);

    // Close Tab 1 (currently viewed by pane 1) -> pane 1 clamps to remaining valid tab (0)
    app.close_tab_and_sync_splits(1);
    assert_eq!(app.splits.panes[1].tab_idx, 0);
}

#[test]
fn test_shared_buffer_live_editing_across_splits() {
    let mut app = App::new(None).unwrap();
    app.terminal_width = 80;
    app.terminal_height = 24;

    // Pane 0 and Pane 1 both view Tab 0
    app.splits
        .split_vertical(0, (0, 0), app.editor.cursors().clone());
    assert_eq!(app.splits.panes[0].tab_idx, 0);
    assert_eq!(app.splits.panes[1].tab_idx, 0);

    // Edit buffer in active pane
    app.editor.insert_str("shared text").unwrap();

    // Both panes must see the exact same buffer text instantly via Rope B-Tree
    let tab = &app.editor.tabs()[0];
    assert_eq!(tab.buffer.rope().to_string(), "shared text");

    let mut output = Vec::new();
    app.render(&mut output).unwrap();
    let rendered = String::from_utf8_lossy(&output);
    assert!(rendered.contains("shared text"));

    // Verify cursor clamping across shared buffer when text is deleted
    app.switch_active_pane(0);
    app.editor.move_end(false);
    app.sync_active_pane();
    assert_eq!(app.splits.panes[0].cursors.primary().head, 11);

    // Switch back to pane 1 and delete the entire line
    app.switch_active_pane(1);
    app.editor.select_all();
    app.editor.delete_backwards().unwrap();
    app.sync_active_pane();

    // Pane 0's cursor must clamp to 0 safely
    assert_eq!(app.splits.panes[0].cursors.primary().head, 0);
}

#[test]
fn test_inactive_split_pane_syntax_response_triggers_update_and_mouse_clears_chord() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use std::path::PathBuf;
    use std::time::Duration;

    let mut app = App::new(None).unwrap();
    app.explorer.is_visible = false;
    app.terminal_width = 80;
    app.terminal_height = 24;

    // Tab 0 (empty plain text) and Tab 1 (Rust file)
    let tab1_idx = app.editor.new_tab();
    assert_eq!(tab1_idx, 1);
    app.editor.set_file_path(Some(PathBuf::from("lib.rs")));
    app.editor.insert_str("fn main() { let x = 42; }").unwrap();

    // Switch active tab back to Tab 0, split vertically, and set Pane 0 (inactive) to view Tab 1
    app.editor.switch_tab(0);
    app.execute_command("view.split_vertical");
    assert_eq!(app.splits.active_pane, 1);
    app.splits.panes[0].tab_idx = 1;
    app.splits.panes[1].tab_idx = 0;

    // Request syntax highlights for Tab 1 (viewed by inactive Pane 0) while active tab is Tab 0
    let _ = app.update_syntax_highlights_for_tab(1);
    let mut got_update = false;
    for _ in 0..50 {
        if app.has_pending_syntax_response() {
            got_update = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        got_update,
        "Syntax response for visible inactive split pane must mark UI updated"
    );

    // Also verify mouse click cancels pending Ctrl+K chord
    app.pending_chord = Some('k');
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 60,
        row: 5,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();
    assert_eq!(app.pending_chord, None);
}

#[test]
fn test_tab_switching_shortcuts_sync_split_pane_and_viewport() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = App::new(None).unwrap();
    app.explorer.is_visible = false;
    app.terminal_width = 80;
    app.terminal_height = 10;

    // Tab 0 has 30 lines, scrolled down to line 20
    let long_text: String = (0..30).map(|i| format!("line {i}\n")).collect();
    app.editor.insert_str(&long_text).unwrap();
    app.scroll_row = 20;

    // Tab 1 has 1 line (cursor at line 0)
    let _ = app.editor.new_tab();
    app.editor.insert_str("short").unwrap();

    // Switch back to Tab 0 and split vertically (active_pane = 1, viewing Tab 0 at scroll_row = 20)
    app.editor.switch_tab(0);
    app.scroll_row = 20;
    app.execute_command("view.split_vertical");
    assert_eq!(app.splits.active_pane, 1);
    assert_eq!(app.splits.panes[1].tab_idx, 0);
    assert_eq!(app.splits.panes[1].scroll_row, 20);

    // Press Ctrl+Tab to switch active pane to Tab 1
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::CONTROL))
        .unwrap();

    // Active pane's tab_idx must immediately sync to 1 and scroll_row must clamp to 0
    assert_eq!(app.editor.active_tab_idx(), 1);
    assert_eq!(app.splits.panes[1].tab_idx, 1);
    assert_eq!(app.scroll_row, 0);
    assert_eq!(app.splits.panes[1].scroll_row, 0);

    // Press Alt+Left to switch back to Tab 0
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT))
        .unwrap();
    assert_eq!(app.editor.active_tab_idx(), 0);
    assert_eq!(app.splits.panes[1].tab_idx, 0);
}
