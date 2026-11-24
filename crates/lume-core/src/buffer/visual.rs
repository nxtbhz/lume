use ropey::Rope;
use unicode_segmentation::{GraphemeCursor, GraphemeIncomplete, UnicodeSegmentation};
use unicode_width::UnicodeWidthStr;

pub struct Grapheme<'a> {
    pub byte_offset: usize,
    pub text: &'a str,
    pub column: usize,
    pub width: usize,
}

impl Grapheme<'_> {
    pub fn display_text(&self) -> &str {
        if self.text == "\t" {
            &"    "[..self.width]
        } else if starts_with_control(self.text) {
            "�"
        } else {
            self.text
        }
    }
}

fn starts_with_control(text: &str) -> bool {
    text.chars().next().is_some_and(char::is_control)
}

/// Maps UTF-8 graphemes to terminal columns, expanding four-column tabs.
pub fn graphemes(text: &str) -> impl Iterator<Item = Grapheme<'_>> {
    graphemes_from(text, 0)
}

pub fn graphemes_from(text: &str, column: usize) -> impl Iterator<Item = Grapheme<'_>> {
    text.grapheme_indices(true)
        .scan(column, |column, (byte_offset, text)| {
            let width = if text == "\t" {
                4 - *column % 4
            } else if starts_with_control(text) {
                1
            } else {
                text.width()
            };
            let grapheme = Grapheme {
                byte_offset,
                text,
                column: *column,
                width,
            };
            *column += width;
            Some(grapheme)
        })
}

/// A byte offset inside a grapheme maps to its first terminal column.
pub fn column_at_byte(text: &str, byte_offset: usize) -> usize {
    graphemes(text)
        .take_while(|g| g.byte_offset + g.text.len() <= byte_offset)
        .map(|g| g.width)
        .sum()
}

/// A column inside a grapheme maps to its first byte.
pub fn byte_at_column(text: &str, column: usize) -> usize {
    graphemes(text)
        .find(|g| column < g.column + g.width)
        .map_or(text.len(), |g| g.byte_offset)
}

pub(crate) fn previous_boundary(rope: &Rope, byte: usize) -> usize {
    adjacent_boundary(rope, byte, false)
}

pub(crate) fn next_boundary(rope: &Rope, byte: usize) -> usize {
    adjacent_boundary(rope, byte, true)
}

fn adjacent_boundary(rope: &Rope, byte: usize, forward: bool) -> usize {
    let mut cursor = GraphemeCursor::new(byte, rope.len_bytes(), true);
    let lookup = if forward {
        byte
    } else {
        byte.saturating_sub(1)
    };
    let (mut chunk, mut start, _, _) = rope.chunk_at_byte(lookup);
    loop {
        let boundary = if forward {
            cursor.next_boundary(chunk, start)
        } else {
            cursor.prev_boundary(chunk, start)
        };
        match boundary {
            Ok(boundary) => return boundary.unwrap_or(byte),
            Err(GraphemeIncomplete::NextChunk) => {
                (chunk, start, _, _) = rope.chunk_at_byte(start + chunk.len());
            }
            Err(GraphemeIncomplete::PrevChunk) => {
                (chunk, start, _, _) = rope.chunk_at_byte(start - 1);
            }
            Err(GraphemeIncomplete::PreContext(end)) => {
                let (context, context_start, _, _) = rope.chunk_at_byte(end - 1);
                cursor.provide_context(&context[..end - context_start], context_start);
            }
            Err(GraphemeIncomplete::InvalidOffset) => {
                unreachable!("grapheme cursor offset must be within the requested rope chunk");
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WrapSegment {
    pub start_byte: usize,
    pub end_byte: usize,
    pub col_width: usize,
}

pub struct WrapIter<'a> {
    text: &'a str,
    pos: usize,
    width: usize,
    empty: bool,
}

pub fn wrap_cols_for_viewport(text_width: usize) -> usize {
    text_width.saturating_sub(1).max(1)
}

pub fn wrap_segments(text: &str, max_cols: usize) -> WrapIter<'_> {
    WrapIter {
        text,
        pos: 0,
        width: max_cols.max(1),
        empty: text.is_empty(),
    }
}

impl Iterator for WrapIter<'_> {
    type Item = WrapSegment;
    fn next(&mut self) -> Option<Self::Item> {
        let start = self.pos;
        if start == self.text.len() && !std::mem::take(&mut self.empty) {
            return None;
        }
        let mut space = None;
        let mut end = start;
        let mut width = 0;
        for g in graphemes(&self.text[start..]) {
            let next = g.column + g.width;
            if next > self.width && g.byte_offset > 0 {
                if let Some(boundary) = space {
                    (end, width) = boundary;
                }
                break;
            }
            end = start + g.byte_offset + g.text.len();
            width = next;
            if width > self.width {
                break;
            }
            if matches!(g.text, " " | "\t") {
                space = Some((end, width));
            }
        }
        self.pos = end;
        Some(WrapSegment {
            start_byte: start,
            end_byte: end,
            col_width: width,
        })
    }
}

pub fn wrap_subrow_count(text: &str, max_cols: usize) -> usize {
    wrap_segments(text, max_cols).count()
}

pub fn cursor_subrow_and_col(text: &str, byte_offset: usize, max_cols: usize) -> (usize, usize) {
    for (row, s) in wrap_segments(text, max_cols).enumerate() {
        if byte_offset < s.end_byte || s.end_byte == text.len() {
            return (
                row,
                column_at_byte(
                    &text[s.start_byte..s.end_byte],
                    byte_offset.saturating_sub(s.start_byte),
                ),
            );
        }
    }
    unreachable!("each line has a final segment")
}

pub fn byte_at_subrow_col(
    text: &str,
    target_subrow: usize,
    target_col: usize,
    max_cols: usize,
) -> usize {
    for (row, s) in wrap_segments(text, max_cols).enumerate() {
        if row == target_subrow || s.end_byte == text.len() {
            let part = &text[s.start_byte..s.end_byte];
            if target_col >= s.col_width && s.end_byte < text.len() {
                return s.start_byte + part.grapheme_indices(true).next_back().unwrap().0;
            }
            return s.start_byte + byte_at_column(part, target_col);
        }
    }
    unreachable!("each line has a final segment")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizontal_boundaries_match_contiguous_text_across_rope_chunks() {
        let text = format!("{}\r\n{}", "a".repeat(1018), "👩‍💻e\u{301}🇧🇷".repeat(300));
        let rope = Rope::from_str(&text);
        assert!(rope.chunks().count() > 1);
        let boundaries: Vec<_> = text
            .grapheme_indices(true)
            .map(|(byte, _)| byte)
            .chain(std::iter::once(text.len()))
            .collect();
        for pair in boundaries.windows(2) {
            assert_eq!(next_boundary(&rope, pair[0]), pair[1]);
            assert_eq!(previous_boundary(&rope, pair[1]), pair[0]);
        }
    }
}

#[cfg(test)]
mod wrap_tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn wrap_prefers_whitespace_and_falls_back_to_grapheme() {
        let ranges: Vec<_> = wrap_segments("hello world superlongword", 10)
            .map(|s| (s.start_byte, s.end_byte, s.col_width))
            .collect();
        assert_eq!(ranges, [(0, 6, 6), (6, 12, 6), (12, 22, 10), (22, 25, 3)]);
    }
    #[test]
    fn wrap_boundary_and_non_final_click_stay_on_intended_subrow() {
        let s = "hello world";
        assert_eq!(cursor_subrow_and_col(s, 6, 6), (1, 0));
        assert_eq!(cursor_subrow_and_col(s, 11, 6), (1, 5));
        assert_eq!(byte_at_subrow_col(s, 0, 99, 6), 5);
        assert_eq!(byte_at_subrow_col(s, 1, 99, 6), 11);
    }
    proptest! {
        #[test]
        fn proptest_wrap_partition_and_roundtrip_unicode_tabs_narrow(s in "(a| |界|👩‍💻|é|́|\t|\x01){0,40}", width in 1usize..=40) {
            let mut end = 0;
            prop_assert!(wrap_segments(&s,width).count() >= 1);
            for (row, seg) in wrap_segments(&s,width).enumerate() {
                prop_assert_eq!(seg.start_byte,end);
                end = seg.end_byte;
                let part = &s[seg.start_byte..end];
                prop_assert!(s.is_empty() || !part.is_empty());
                for g in graphemes(part).filter(|g| g.width > 0) {
                    let byte = seg.start_byte + g.byte_offset;
                    prop_assert_eq!(cursor_subrow_and_col(&s,byte,width),(row,g.column));
                    prop_assert_eq!(byte_at_subrow_col(&s,row,g.column,width),byte);
                }
            }
            prop_assert_eq!(end,s.len());
        }
    }
}
