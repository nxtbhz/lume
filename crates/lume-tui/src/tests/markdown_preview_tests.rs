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
