use crate::{app::App, split::PaneViewMode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use lume_core::Editor;
const MD: &str =
    "# Relatório\n\n| Métrica | Valor |\n| :--- | ---: |\n| Latência | **1.98 µs** |\n";
fn app(src: &str) -> App {
    let mut a = App::new(None).unwrap();
    a.explorer.is_visible = false;
    a.terminal_width = 100;
    a.terminal_height = 12;
    a.editor = Editor::from_str(src);
    a.editor.set_file_path(Some("example.md".into()));
    a
}
fn frame(a: &mut App) -> String {
    let mut v = Vec::new();
    a.render(&mut v).unwrap();
    String::from_utf8(v).unwrap()
}
#[test]
fn toggle_preview_renders_aligned_table_and_hides_raw_pipes_and_hashes() {
    let mut a = app(MD);
    a.execute_command("markdown.toggle_preview");
    let s = frame(&mut a);
    for expected in ["┌", "Latência", "1.98 µs", "[PREVIEW]", "\x1b[?25l"] {
        assert!(s.contains(expected), "{expected}");
    }
    for raw in ["# Relatório", "| :--- |", "**1.98 µs**", "\x1b[?25h"] {
        assert!(!s.contains(raw));
    }
}
#[test]
fn split_live_sync_and_preview_read_only_guards() {
    let mut a = app(&format!("{}{}", MD, "more\n".repeat(60)));
    a.execute_command("markdown.open_preview_split");
    assert!(a.splits.is_split);
    assert_eq!(a.splits.active_pane, 0);
    assert_eq!(a.splits.panes[1].view_mode, PaneViewMode::MarkdownPreview);
    a.editor.set_cursor_from_line_col(5, 0, false);
    a.handle_paste_text("| Nova | 2 |\n");
    let s = frame(&mut a);
    assert!(s.matches("Nova").count() >= 2);
    a.switch_active_pane(1);
    let rev = a.editor.active_tab().revision;
    for key in [KeyCode::Char('x'), KeyCode::Backspace, KeyCode::Enter] {
        a.handle_key(KeyEvent::new(key, KeyModifiers::NONE))
            .unwrap();
    }
    a.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL))
        .unwrap();
    a.handle_paste_text("forbidden");
    assert_eq!(rev, a.editor.active_tab().revision);
    for cmd in ["edit.replace", "edit.undo", "vim.open_line_below"] {
        a.execute_command(cmd);
    }
    assert_eq!(rev, a.editor.active_tab().revision);
    for key in [KeyCode::Down, KeyCode::Char('j'), KeyCode::PageDown] {
        let old = a.preview_scroll_row;
        a.handle_key(KeyEvent::new(key, KeyModifiers::NONE))
            .unwrap();
        assert!(a.preview_scroll_row > old);
    }
    let old = a.preview_scroll_row;
    a.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 80,
        row: 4,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();
    assert!(a.preview_scroll_row > old);
    let mut clean = app(MD);
    clean.execute_command("markdown.toggle_preview");
    clean.handle_paste_text("do not edit");
    assert!(!clean.editor.is_dirty());
}

#[test]
#[ignore = "release-mode measurement; set LUME_FRAME_COUNT to choose the sample size"]
fn measure_hot_markdown_frames() {
    let count: usize = std::env::var("LUME_FRAME_COUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let mut a = app(&MD.repeat(100));
    a.execute_command("markdown.toggle_preview");
    let mut bytes = Vec::new();
    a.render(&mut bytes).unwrap();
    let start = std::time::Instant::now();
    for i in 0..count {
        bytes.clear();
        a.scroll_pane_visual_rows(None, if i % 2 == 0 { 1 } else { -1 });
        a.render(&mut bytes).unwrap();
        std::hint::black_box(&bytes);
    }
    eprintln!(
        "frames={count} elapsed_ns={} bytes_last={}",
        start.elapsed().as_nanos(),
        bytes.len()
    );
}

#[test]
fn replacing_and_closing_tabs_releases_cached_markdown() {
    let dir = super::TestDir::new("lume-preview-reuse");
    let path = dir.path().join("loaded.md");
    std::fs::write(&path, "# Loaded file").unwrap();
    let mut a = App::new(None).unwrap();
    a.execute_command("markdown.toggle_preview");
    let width = a.pane_rect(0).width;
    let old = std::sync::Arc::downgrade(&a.preview_layout(0, width));
    a.open_file_into_editor_result(&path).unwrap();
    assert!(old.upgrade().is_none());
    assert!(frame(&mut a).contains("Loaded file"));
    let layout = std::sync::Arc::downgrade(&a.preview_layout(0, width));
    a.close_tab_and_sync_splits(0);
    assert!(layout.upgrade().is_none());
}

#[test]
fn preview_follows_source_wheel_inside_single_long_paragraph() {
    let mut a = app(&"sample text ".repeat(6000));
    a.execute_command("markdown.open_preview_split");
    for _ in 0..4 {
        a.handle_event(
            crate::input::AppEvent::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 4,
                row: 3,
                modifiers: KeyModifiers::NONE,
            }),
            &mut Vec::new(),
        )
        .unwrap();
    }
    assert_eq!(a.scroll_row, 0);
    assert!(a.scroll_subrow >= 10);
    assert!(a.splits.panes[1].preview_scroll_row >= 8);
}
#[test]
fn preview_allows_find_and_copy_and_blocks_replace_in_search() {
    let mut a = app(&format!("{}needle", "other\n".repeat(40)));
    a.execute_command("markdown.toggle_preview");
    a.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(a.search_state.is_some());
    for c in "needle".chars() {
        a.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }
    assert!(a.preview_scroll_row > 0);
    a.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(matches!(
        a.search_state.as_ref().unwrap().mode,
        crate::search::SearchMode::Find
    ));
    a.search_state = None;
    a.execute_command("edit.copy");
    assert_eq!(a.editor.clipboard(), Some("needle"));
}
