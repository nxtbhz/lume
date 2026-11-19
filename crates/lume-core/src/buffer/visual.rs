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
