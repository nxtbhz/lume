use super::*;
use layout::*;
use std::sync::Arc;

fn text(row: &PreviewRow) -> String {
    row.spans.iter().map(|s| s.text.as_str()).collect()
}

#[test]
fn table_inline_code_pipes_escaped_pipes_ragged_cells_and_narrow_fallback() {
    let doc=parse_markdown("| `a | b` | x \\| y | C |\n| :--- | :---: | ---: |\n| only_one |\n| 界界 | abc | a long description which must continue inside the cell |\n");
    let MarkdownBlock::Table { headers, rows, .. } = &doc.blocks[0] else {
        panic!("expected table")
    };
    assert_eq!(headers.len(), 3);
    assert_eq!(headers[0][0].text, "a | b");
    assert!(headers[0][0].style.code);
    assert_eq!(headers[1][0].text, "x | y");
    assert_eq!(rows[0].len(), 3);
    for w in 1..=200 {
        let layout = layout_markdown(&doc, w, 5);
        assert!(layout
            .rows
            .iter()
            .all(|r| r.width <= w
                && unicode_width::UnicodeWidthStr::width(text(r).as_str()) == r.width));
        let flat = layout.rows.iter().map(text).collect::<Vec<_>>().join("\n");
        if w >= 14 {
            assert!("┌┬┐├┼┤└┴┘".chars().all(|c| flat.contains(c)));
            let grid: Vec<_> = layout
                .rows
                .iter()
                .map(text)
                .filter(|s| s.starts_with('│'))
                .collect();
            let bars = |s: &str| {
                lume_core::buffer::visual::graphemes(s)
                    .filter(|g| g.text == "│")
                    .map(|g| g.column)
                    .collect::<Vec<_>>()
            };
            assert!(grid.iter().all(|s| bars(s) == bars(&grid[0])));
        } else {
            assert!(!flat.contains('┌'));
            if w == 10 {
                assert!(flat.contains("only_one"));
            }
        }
    }
}
#[test]
fn gfm_blocks_headings_alerts_tasklists_links_and_fenced_code() {
    let doc=parse_markdown("# H1\n> [!WARNING]\n> cuidado\n- [x] pronto\n[docs](https://example.com) **bold *both*** ~~strike~~\n```rust\nfn main() {}\n```");
    let layout = layout_markdown(&doc, 72, 8);
    let flat = layout.rows.iter().map(text).collect::<Vec<_>>().join("\n");
    for s in [
        "H1",
        "WARNING",
        "cuidado",
        "☑ pronto",
        "docs",
        "rust",
        "fn main()",
    ] {
        assert!(flat.contains(s), "{flat}");
    }
    for s in ["# H1", "[!", "**", "https:", "```"] {
        assert!(!flat.contains(s));
    }
    let spans: Vec<_> = layout.rows.iter().flat_map(|r| &r.spans).collect();
    assert!(spans
        .iter()
        .any(|s| s.text.contains("both") && s.style.bold && s.style.italic));
    assert!(spans.iter().any(|s| s.text == "docs" && s.style.underlined));
    assert!(spans
        .iter()
        .any(|s| s.text == "strike" && s.style.crossed_out));
    assert!(spans
        .iter()
        .any(|s| s.text == "fn" && s.style.fg == crossterm::style::Color::Cyan));
}
#[test]
fn cache_reuses_ast_on_resize_and_reuses_layout_on_hot_scroll() {
    let rope = ropey::Rope::from_str("# hello");
    let mut c = MarkdownPreviewCache::default();
    let a = c.get_or_compute(1, 0, &rope, 20);
    let doc = c.tabs[&1].doc.clone();
    assert!(Arc::ptr_eq(&a, &c.get_or_compute(1, 0, &rope, 20)));
    for w in 21..99 {
        c.get_or_compute(1, 0, &rope, w);
    }
    assert!(Arc::ptr_eq(&doc, &c.tabs[&1].doc));
    assert_eq!(c.tabs[&1].layouts.len(), 2);
    assert!(!Arc::ptr_eq(&a, &c.get_or_compute(1, 1, &rope, 20)));
    assert!(!Arc::ptr_eq(&doc, &c.tabs[&1].doc));
    c.invalidate_tab(1);
    assert!(c.tabs.is_empty());
}

#[test]
fn narrow_grid_preserves_wide_graphemes_when_stacked_cells_can_fit_them() {
    let doc = parse_markdown("| a | b |\n|---|---|\n|界|🙂|");
    for width in 2..18 {
        let layout = layout_markdown(&doc, width, 3);
        let visible: String = layout.rows.iter().map(text).collect();
        assert!(
            visible.contains('界') && visible.contains('🙂'),
            "width={width} {visible}"
        );
        assert!(layout.rows.iter().all(|r| r.width <= width));
    }
}

#[test]
fn unmatched_markup_stays_literal_and_only_matched_ticks_protect_pipes() {
    let doc =
        parse_markdown("| H | V |\n|---|---|\n| stray ` here | value |\n| ``a ` b | c`` | end |");
    let MarkdownBlock::Table { rows, .. } = &doc.blocks[0] else {
        panic!("table")
    };
    assert_eq!(rows[0][0][0].text, "stray ` here");
    assert_eq!(rows[0][1][0].text, "value");
    assert_eq!(rows[1][0][0].text, "a ` b | c");
    assert!(rows[1][0][0].style.code);
}

#[test]
fn unmatched_brackets_do_not_steal_a_later_link() {
    let prefix = "[".repeat(65536);
    let spans = inline(&format!("{prefix} [docs](target)"));
    assert_eq!(spans[0].text, format!("{prefix} "));
    assert!(!spans[0].style.link);
    assert_eq!(spans[1].text, "docs");
    assert!(spans[1].style.link);
}
