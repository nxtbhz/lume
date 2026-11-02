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
