#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineEnding {
    #[default]
    Lf,
    CrLf,
}

impl LineEnding {
    /// Detects the line ending style from the first line break in `rope`.
    /// Defaults to `LineEnding::Lf` when no newline is present or on orphan `\r`.
    pub fn detect(rope: &ropey::Rope) -> Self {
        let mut prev_ended_with_cr = false;
        for chunk in rope.chunks() {
            let bytes = chunk.as_bytes();
            if let Some(pos) = bytes.iter().position(|&b| b == b'\n') {
                let preceded_by_cr = if pos > 0 {
                    bytes[pos - 1] == b'\r'
                } else {
                    prev_ended_with_cr
                };
                return if preceded_by_cr { Self::CrLf } else { Self::Lf };
            }
            if let Some(&last) = bytes.last() {
                prev_ended_with_cr = last == b'\r';
            }
        }
        Self::Lf
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Lf => "LF",
            Self::CrLf => "CRLF",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LineEnding;

    #[test]
    fn test_detect_empty_and_no_newlines_defaults_to_lf() {
        assert_eq!(LineEnding::detect(&ropey::Rope::new()), LineEnding::Lf);
        assert_eq!(
            LineEnding::detect(&ropey::Rope::from_str("single line")),
            LineEnding::Lf
        );
    }

    #[test]
    fn test_detect_crlf_across_rope_chunk_boundary() {
        // Ropey internal chunk max size is 1024 bytes; place '\r' right before a chunk boundary.
        let mut content = "a".repeat(2047);
        content.push_str("\r\nnext");
        let rope = ropey::Rope::from_str(&content);
        assert!(rope.chunks().count() >= 2);
        assert_eq!(LineEnding::detect(&rope), LineEnding::CrLf);
    }

    #[test]
    fn test_detect_orphan_cr_without_lf_is_lf() {
        assert_eq!(
            LineEnding::detect(&ropey::Rope::from_str("a\rb\r")),
            LineEnding::Lf
        );
    }
}
