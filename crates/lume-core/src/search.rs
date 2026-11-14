use crate::buffer::{RopeBuffer, TextBuffer};

/// Find all non-overlapping occurrences of `needle` in `haystack`.
/// Returns a vector of `(start_byte, end_byte)` tuples.
pub fn find_matches_in_str(
    haystack: &str,
    needle: &str,
    case_sensitive: bool,
) -> Vec<(usize, usize)> {
    if needle.is_empty() || haystack.is_empty() {
        return Vec::new();
    }

    if case_sensitive {
        if needle.len() > haystack.len() {
            return Vec::new();
        }
        let mut matches = Vec::new();
        let needle_len = needle.len();
        let mut start = 0;
        while let Some(pos) = haystack[start..].find(needle) {
            let match_start = start + pos;
            let match_end = match_start + needle_len;
            matches.push((match_start, match_end));
            start = match_end;
        }
        return matches;
    }

    // ASCII fast path
    if haystack.is_ascii() && needle.is_ascii() {
        if needle.len() > haystack.len() {
            return Vec::new();
        }
        let lower_haystack = haystack.to_ascii_lowercase();
        let lower_needle = needle.to_ascii_lowercase();
        let needle_len = lower_needle.len();
        let mut matches = Vec::new();
        let mut start = 0;
        while let Some(pos) = lower_haystack[start..].find(&lower_needle) {
            let match_start = start + pos;
            let match_end = match_start + needle_len;
            matches.push((match_start, match_end));
            start = match_end;
        }
        return matches;
    }

    // General Unicode case-insensitive matching preserving multi-char lowercase expansions (e.g. 'İ' -> "i\u{0307}")
    let needle_folded: Vec<char> = needle.chars().flat_map(|c| c.to_lowercase()).collect();
    if needle_folded.is_empty() {
        return Vec::new();
    }

    let mut h_folded: Vec<char> = Vec::with_capacity(haystack.len());
    let mut h_spans: Vec<(usize, usize)> = Vec::with_capacity(haystack.len());
    for (orig_start, ch) in haystack.char_indices() {
        let orig_end = orig_start + ch.len_utf8();
        for lc in ch.to_lowercase() {
            h_folded.push(lc);
            h_spans.push((orig_start, orig_end));
        }
    }

    let n_len = needle_folded.len();
    if n_len > h_folded.len() {
        return Vec::new();
    }

    let mut matches = Vec::new();
    let mut i = 0;
    while i + n_len <= h_folded.len() {
        // Align start and end strictly on whole codepoint boundaries in `haystack`
        if i > 0 && h_spans[i].0 == h_spans[i - 1].0 {
            i += 1;
            continue;
        }
        let end_idx = i + n_len;
        if end_idx < h_spans.len() && h_spans[end_idx - 1].1 == h_spans[end_idx].1 {
            i += 1;
            continue;
        }
        if h_folded[i..end_idx] == needle_folded[..] {
            matches.push((h_spans[i].0, h_spans[end_idx - 1].1));
            i = end_idx;
        } else {
            i += 1;
        }
    }

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
    let actual_start = buffer.char_to_byte(buffer.byte_to_char(clamped_start));
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

/// Count total non-overlapping occurrences of `needle` in `haystack` without extra allocations.
pub fn count_matches_in_str(haystack: &str, needle: &str, case_sensitive: bool) -> usize {
    if needle.is_empty() || haystack.is_empty() || needle.len() > haystack.len() {
        return 0;
    }

    if case_sensitive {
        let mut count = 0;
        let mut start = 0;
        while let Some(pos) = haystack[start..].find(needle) {
            count += 1;
            start += pos + needle.len();
        }
        return count;
    }

    if haystack.is_ascii() && needle.is_ascii() {
        let lower_haystack = haystack.to_ascii_lowercase();
        let lower_needle = needle.to_ascii_lowercase();
        let mut count = 0;
        let mut start = 0;
        while let Some(pos) = lower_haystack[start..].find(&lower_needle) {
            count += 1;
            start += pos + lower_needle.len();
        }
        return count;
    }

    find_matches_in_str(haystack, needle, case_sensitive).len()
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
}
