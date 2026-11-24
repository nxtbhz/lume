use crate::buffer::{RopeBuffer, TextBuffer};

#[inline]
fn find_ascii_first_byte(bytes: &[u8], first: u8, fold_case: bool) -> Option<usize> {
    const LO: u64 = 0x0101_0101_0101_0101;
    const HI: u64 = 0x8080_8080_8080_8080;
    const CASE_BIT: u64 = 0x2020_2020_2020_2020;

    let case_mask = if fold_case { CASE_BIT } else { 0 };
    let target = if fold_case { first | 0x20 } else { first };
    let splat = u64::from_le_bytes([target; 8]);

    let (chunks, rem) = bytes.as_chunks::<8>();
    for (idx, chunk) in chunks.iter().enumerate() {
        let word = u64::from_le_bytes(*chunk) | case_mask;
        let x = word ^ splat;
        let has = x.wrapping_sub(LO) & !x & HI;
        if has != 0 {
            return Some(idx * 8 + (has.trailing_zeros() as usize >> 3));
        }
    }
    let tail_offset = chunks.len() * 8;
    let mask = if fold_case { 0x20 } else { 0 };
    rem.iter()
        .position(|&b| (b | mask) == target)
        .map(|j| tail_offset + j)
}

fn match_unicode_at(slice: &str, needle_folded: &[char]) -> Option<usize> {
    let mut k = 0;
    for (byte_offset, ch) in slice.char_indices() {
        let ch_end = byte_offset + ch.len_utf8();
        if ch.is_ascii() {
            if ch.to_ascii_lowercase() != needle_folded[k] {
                return None;
            }
            k += 1;
            if k == needle_folded.len() {
                return Some(ch_end);
            }
        } else {
            for lc in ch.to_lowercase() {
                if k >= needle_folded.len() || lc != needle_folded[k] {
                    return None;
                }
                k += 1;
            }
            if k == needle_folded.len() {
                return Some(ch_end);
            }
        }
    }
    None
}

pub fn for_each_match_in_str(
    haystack: &str,
    needle: &str,
    case_sensitive: bool,
    mut on_match: impl FnMut(usize, usize),
) {
    if needle.is_empty() || haystack.is_empty() {
        return;
    }

    if case_sensitive {
        if needle.len() > haystack.len() {
            return;
        }
        let n_len = needle.len();
        for (start, _) in haystack.match_indices(needle) {
            on_match(start, start + n_len);
        }
        return;
    }

    if haystack.is_ascii() && needle.is_ascii() {
        let h = haystack.as_bytes();
        let n = needle.as_bytes();
        let n_len = n.len();
        if n_len > h.len() {
            return;
        }
        let first = n[0];
        let fold = first.is_ascii_alphabetic();
        let tail = &n[1..];
        let limit = h.len() - n_len;
        let mut i = 0;
        while i <= limit {
            let Some(rel) = find_ascii_first_byte(&h[i..=limit], first, fold) else {
                break;
            };
            i += rel;
            if n_len == 1 || h[i + 1..i + n_len].eq_ignore_ascii_case(tail) {
                on_match(i, i + n_len);
                i += n_len;
            } else {
                i += 1;
            }
        }
        return;
    }

    // Streaming Unicode case-insensitive match preserving multi-char lowercase expansions (e.g. 'İ' -> "i\u{0307}")
    let mut stack_buf = ['\0'; 32];
    let mut heap_buf = Vec::new();
    let mut n_len = 0;
    for ch in needle.chars().flat_map(|c| c.to_lowercase()) {
        if n_len < stack_buf.len() && heap_buf.is_empty() {
            stack_buf[n_len] = ch;
        } else {
            if heap_buf.is_empty() {
                heap_buf.extend_from_slice(&stack_buf[..n_len]);
            }
            heap_buf.push(ch);
        }
        n_len += 1;
    }
    let needle_folded = if heap_buf.is_empty() {
        &stack_buf[..n_len]
    } else {
        &heap_buf[..]
    };
    if needle_folded.is_empty() {
        return;
    }

    let bytes = haystack.as_bytes();
    let first_lc = needle_folded[0];
    let first_ascii_lower = first_lc.is_ascii().then_some(first_lc as u8);
    let mut pos = 0;
    while pos < bytes.len() {
        while pos < bytes.len() && bytes[pos] < 0x80 {
            if Some(bytes[pos].to_ascii_lowercase()) == first_ascii_lower {
                if needle_folded.len() == 1 {
                    on_match(pos, pos + 1);
                    pos += 1;
                    continue;
                }
                if let Some(matched_len) = match_unicode_at(&haystack[pos..], needle_folded) {
                    on_match(pos, pos + matched_len);
                    pos += matched_len;
                    continue;
                }
            }
            pos += 1;
        }
        if pos >= bytes.len() {
            break;
        }
        let rest = &haystack[pos..];
        let Some(ch) = rest.chars().next() else {
            break;
        };
        let ch_len = ch.len_utf8();
        if ch.to_lowercase().next() == Some(first_lc) {
            if let Some(matched_len) = match_unicode_at(rest, needle_folded) {
                on_match(pos, pos + matched_len);
                pos += matched_len;
                continue;
            }
        }
        pos += ch_len;
    }
}

/// Find all non-overlapping occurrences of `needle` in `haystack`.
/// Returns a vector of `(start_byte, end_byte)` tuples.
pub fn find_matches_in_str(
    haystack: &str,
    needle: &str,
    case_sensitive: bool,
) -> Vec<(usize, usize)> {
    let mut matches = Vec::new();
    for_each_match_in_str(haystack, needle, case_sensitive, |s, e| {
        matches.push((s, e))
    });
    matches
}

/// Find all non-overlapping occurrences within a specified byte range of `buffer`.
pub fn find_matches_in_range(
    buffer: &RopeBuffer,
    needle: &str,
    start_byte: usize,
    end_byte: usize,
    case_sensitive: bool,
) -> Vec<(usize, usize)> {
    let len = buffer.len_bytes();
    let clamped_start = start_byte.min(len);
    let clamped_end = end_byte.min(len);
    if needle.is_empty() || clamped_start >= clamped_end {
        return Vec::new();
    }
    let start_char = buffer.byte_to_char(clamped_start);
    let mut actual_start = buffer.char_to_byte(start_char);
    if actual_start < clamped_start {
        actual_start = buffer.char_to_byte(start_char + 1);
    }
    if actual_start >= clamped_end {
        return Vec::new();
    }
    let slice = buffer.slice_to_cow(actual_start, clamped_end);
    find_matches_in_str(&slice, needle, case_sensitive)
        .into_iter()
        .map(|(s, e)| (actual_start + s, actual_start + e))
        .collect()
}

fn line_matches(
    buffer: &RopeBuffer,
    line_idx: usize,
    needle: &str,
    case_sensitive: bool,
) -> Vec<(usize, usize)> {
    if let Some(line) = buffer.line(line_idx) {
        let line_start_byte = buffer.line_to_byte(line_idx);
        let s = if let Some(as_str) = line.as_str() {
            std::borrow::Cow::Borrowed(as_str)
        } else {
            std::borrow::Cow::Owned(line.to_string())
        };
        find_matches_in_str(&s, needle, case_sensitive)
            .into_iter()
            .map(|(s, e)| (line_start_byte + s, line_start_byte + e))
            .collect()
    } else {
        Vec::new()
    }
}

/// Find the next occurrence of `needle` at or after `from_byte`.
/// Wraps around to the start of the buffer if reaching EOF.
pub fn find_next(
    buffer: &RopeBuffer,
    needle: &str,
    from_byte: usize,
    case_sensitive: bool,
) -> Option<(usize, usize)> {
    if needle.is_empty() || buffer.len_bytes() == 0 {
        return None;
    }

    if needle.contains('\n') {
        let text = buffer.slice_to_cow(0, buffer.len_bytes());
        let matches = find_matches_in_str(&text, needle, case_sensitive);
        if matches.is_empty() {
            return None;
        }
        for (s, e) in &matches {
            if *s >= from_byte {
                return Some((*s, *e));
            }
        }
        return matches.first().copied();
    }

    let total_lines = buffer.len_lines();
    let start_line = buffer.byte_to_line(from_byte);

    // 1. Search forward from start_line to end of file
    for line in start_line..total_lines {
        let matches = line_matches(buffer, line, needle, case_sensitive);
        for (s, e) in matches {
            if s >= from_byte {
                return Some((s, e));
            }
        }
    }

    // 2. Wrap around from line 0 to start_line
    for line in 0..=start_line {
        let matches = line_matches(buffer, line, needle, case_sensitive);
        for (s, e) in matches {
            if s < from_byte {
                return Some((s, e));
            }
        }
    }

    None
}

/// Find the previous occurrence of `needle` strictly before `from_byte`.
/// Wraps around to the end of the buffer if reaching BOF.
pub fn find_prev(
    buffer: &RopeBuffer,
    needle: &str,
    from_byte: usize,
    case_sensitive: bool,
) -> Option<(usize, usize)> {
    if needle.is_empty() || buffer.len_bytes() == 0 {
        return None;
    }

    if needle.contains('\n') {
        let text = buffer.slice_to_cow(0, buffer.len_bytes());
        let matches = find_matches_in_str(&text, needle, case_sensitive);
        if matches.is_empty() {
            return None;
        }
        for (s, e) in matches.iter().rev() {
            if *s < from_byte {
                return Some((*s, *e));
            }
        }
        return matches.last().copied();
    }

    let total_lines = buffer.len_lines();
    let start_line = buffer
        .byte_to_line(from_byte)
        .min(total_lines.saturating_sub(1));

    // 1. Search backward from start_line to line 0
    for line in (0..=start_line).rev() {
        let matches = line_matches(buffer, line, needle, case_sensitive);
        for (s, e) in matches.into_iter().rev() {
            if s < from_byte {
                return Some((s, e));
            }
        }
    }

    // 2. Wrap around from EOF to start_line
    for line in (start_line..total_lines).rev() {
        let matches = line_matches(buffer, line, needle, case_sensitive);
        for (s, e) in matches.into_iter().rev() {
            if s >= from_byte {
                return Some((s, e));
            }
        }
    }

    None
}

/// Count total non-overlapping occurrences of `needle` in `haystack` without extra heap allocations.
pub fn count_matches_in_str(haystack: &str, needle: &str, case_sensitive: bool) -> usize {
    let mut count = 0;
    for_each_match_in_str(haystack, needle, case_sensitive, |_, _| count += 1);
    count
}

/// Count total non-overlapping occurrences of `needle` in `buffer`.
pub fn count_matches(buffer: &RopeBuffer, needle: &str, case_sensitive: bool) -> usize {
    if needle.is_empty() || buffer.len_bytes() == 0 {
        return 0;
    }

    if needle.contains('\n') {
        let text = buffer.slice_to_cow(0, buffer.len_bytes());
        return count_matches_in_str(&text, needle, case_sensitive);
    }

    let mut count = 0;
    for line_idx in 0..buffer.len_lines() {
        if let Some(line) = buffer.line(line_idx) {
            if let Some(as_str) = line.as_str() {
                count += count_matches_in_str(as_str, needle, case_sensitive);
            } else {
                let s = line.to_string();
                count += count_matches_in_str(&s, needle, case_sensitive);
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::RopeBuffer;

    #[test]
    fn test_find_in_str_basic_and_case() {
        let text = "Hello world, hello Rust!";
        let matches = find_matches_in_str(text, "hello", false);
        assert_eq!(matches, vec![(0, 5), (13, 18)]);

        let exact = find_matches_in_str(text, "Hello", true);
        assert_eq!(exact, vec![(0, 5)]);
    }

    #[test]
    fn test_find_multibyte_utf8() {
        let text = "Função de busca com emoji 🚀 e acentuação!";
        let matches = find_matches_in_str(text, "busca", false);
        assert_eq!(matches.len(), 1);
        let (s, e) = matches[0];
        assert_eq!(&text[s..e], "busca");

        let emoji_matches = find_matches_in_str(text, "🚀", false);
        assert_eq!(emoji_matches.len(), 1);
        let (es, ee) = emoji_matches[0];
        assert_eq!(&text[es..ee], "🚀");
    }

    #[test]
    fn test_find_next_and_prev_with_wraparound() {
        let buf = RopeBuffer::from_str("apple banana apple cherry apple");
        // Occurrences of "apple" at: 0..5, 13..18, 26..31

        assert_eq!(find_next(&buf, "apple", 0, true), Some((0, 5)));
        assert_eq!(find_next(&buf, "apple", 5, true), Some((13, 18)));
        assert_eq!(find_next(&buf, "apple", 18, true), Some((26, 31)));
        // Wrap around at EOF
        assert_eq!(find_next(&buf, "apple", 31, true), Some((0, 5)));

        // Backward search
        assert_eq!(find_prev(&buf, "apple", 31, true), Some((26, 31)));
        assert_eq!(find_prev(&buf, "apple", 26, true), Some((13, 18)));
        assert_eq!(find_prev(&buf, "apple", 13, true), Some((0, 5)));
        // Wrap around at BOF
        assert_eq!(find_prev(&buf, "apple", 0, true), Some((26, 31)));
    }

    #[test]
    fn test_find_empty_query() {
        let buf = RopeBuffer::from_str("sample text");
        assert_eq!(find_next(&buf, "", 0, false), None);
        assert_eq!(find_prev(&buf, "", 0, false), None);
        assert_eq!(count_matches(&buf, "", false), 0);
    }

    #[test]
    fn test_multiline_and_case_sensitive_search() {
        let buf = RopeBuffer::from_str("hello\nworld\nhello\nworld");
        // Multiline search forward
        assert_eq!(find_next(&buf, "hello\nworld", 0, true), Some((0, 11)));
        assert_eq!(find_next(&buf, "hello\nworld", 11, true), Some((12, 23)));
        // Wraparound multiline search forward
        assert_eq!(find_next(&buf, "hello\nworld", 23, true), Some((0, 11)));

        // Multiline search backward
        assert_eq!(find_prev(&buf, "hello\nworld", 23, true), Some((12, 23)));
        assert_eq!(find_prev(&buf, "hello\nworld", 12, true), Some((0, 11)));
        // Wraparound multiline search backward
        assert_eq!(find_prev(&buf, "hello\nworld", 0, true), Some((12, 23)));

        // Not found multiline
        assert_eq!(find_next(&buf, "not\nfound", 0, true), None);
        assert_eq!(find_prev(&buf, "not\nfound", 0, true), None);

        // Case-sensitive count
        assert_eq!(count_matches_in_str("Foo foo FOO foo", "foo", true), 2);
        assert_eq!(count_matches_in_str("Foo foo FOO foo", "foo", false), 4);
        assert_eq!(count_matches_in_str("", "foo", true), 0);
        assert_eq!(count_matches_in_str("foo", "toolongneedle", true), 0);
    }

    #[test]
    fn test_search_unicode_case_insensitive_multichar_expansion() {
        // '\u{0130}' ('İ') folds to "i\u{0307}" (2 chars, 3 UTF-8 bytes)
        assert_eq!(
            find_matches_in_str("A\u{0130}B", "\u{0130}", false),
            vec![(1, 3)]
        );
        assert_eq!(
            find_matches_in_str("A\u{0130}B", "i\u{0307}", false),
            vec![(1, 3)]
        );
        assert_eq!(
            find_matches_in_str("Ai\u{0307}B", "\u{0130}", false),
            vec![(1, 4)]
        );

        let buf = RopeBuffer::from_str("\u{0130}b");
        assert_eq!(
            find_matches_in_range(&buf, "b", 1, buf.len_bytes(), false),
            vec![(2, 3)]
        );
    }

    #[test]
    fn test_unicode_count_and_range_start_boundary() {
        assert_eq!(
            count_matches_in_str("Ai\u{0307}B \u{0130}", "\u{0130}", false),
            2
        );
        assert_eq!(count_matches_in_str("Ai\u{0307}B", "i\u{0307}", false), 1);
        assert_eq!(
            count_matches_in_str("\u{0130} \u{0130} i\u{0307}", "I\u{0307}", false),
            3
        );

        let buf = RopeBuffer::from_str("éclair");
        // Byte 1 is inside 2-byte 'é' (0..2): must round forward to byte 2, not backward to 0.
        assert_eq!(
            find_matches_in_range(&buf, "éclair", 1, buf.len_bytes(), false),
            vec![]
        );
        assert_eq!(
            find_matches_in_range(&buf, "c", 1, buf.len_bytes(), false),
            vec![(2, 3)]
        );
    }
}
