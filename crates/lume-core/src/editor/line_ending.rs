/// Scans for `b'\n'` 8 bytes at a time via SWAR bit-twiddling in safe Rust
/// so LLVM does not degrade chunk search into a 1-byte-per-iteration scalar loop.
#[inline]
fn find_lf(bytes: &[u8]) -> Option<usize> {
    const LO: u64 = 0x0101_0101_0101_0101;
    const HI: u64 = 0x8080_8080_8080_8080;
    const MASK: u64 = 0x0a0a_0a0a_0a0a_0a0a;

    let (chunks, rem) = bytes.as_chunks::<8>();
    for (i, chunk) in chunks.iter().enumerate() {
        let word = u64::from_le_bytes(*chunk);
        let x = word ^ MASK;
        let has_lf = x.wrapping_sub(LO) & !x & HI;
        if has_lf != 0 {
            return Some(i * 8 + (has_lf.trailing_zeros() as usize >> 3));
        }
    }
    let tail_offset = chunks.len() * 8;
    rem.iter()
        .position(|&b| b == b'\n')
        .map(|j| tail_offset + j)
}

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
            if let Some(pos) = find_lf(bytes) {
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
