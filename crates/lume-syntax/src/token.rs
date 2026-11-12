#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenType {
    Keyword,
    Function,
    Type,
    String,
    Number,
    Comment,
    Operator,
    Macro,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HighlightToken {
    pub start_byte: usize,
    pub end_byte: usize,
    pub token_type: TokenType,
}

impl HighlightToken {
    pub fn new(start_byte: usize, end_byte: usize, token_type: TokenType) -> Self {
        Self {
            start_byte,
            end_byte,
            token_type,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ByteDelta {
    pub start_byte: usize,
    pub old_len: usize,
    pub new_len: usize,
}

impl ByteDelta {
    pub fn new(start_byte: usize, old_len: usize, new_len: usize) -> Self {
        Self {
            start_byte,
            old_len,
            new_len,
        }
    }
}

/// Shifts cached syntax tokens to track text changes before a fresh parse completes.
///
/// Rules:
/// - Tokens strictly before `start_byte` (`token.end_byte <= delta.start_byte`) remain unchanged.
/// - Tokens strictly after the edited range (`token.start_byte >= delta.start_byte + delta.old_len`)
///   shift their `start_byte` and `end_byte` by `new_len as isize - old_len as isize`.
/// - Overlapping tokens (where edit touches or is inside the token) are dropped to prevent
///   stale or mid-UTF-8 char highlighting.
pub fn shift_tokens_for_deltas(
    tokens: &[HighlightToken],
    deltas: &[ByteDelta],
) -> Vec<HighlightToken> {
    let mut result = tokens.to_vec();
    shift_tokens_in_place(&mut result, deltas);
    result
}

/// In-place variant of `shift_tokens_for_deltas` operating on `Vec<HighlightToken>`.
pub fn shift_tokens_in_place(tokens: &mut Vec<HighlightToken>, deltas: &[ByteDelta]) {
    for delta in deltas {
        let edit_start = delta.start_byte;
        let edit_end = delta.start_byte + delta.old_len;
        let diff = delta.new_len as isize - delta.old_len as isize;

        tokens.retain_mut(|token| {
            if token.end_byte <= edit_start {
                // Strictly before edit: keep unchanged
                true
            } else if token.start_byte >= edit_end {
                // Strictly after edit: shift offsets
                let new_start = (token.start_byte as isize + diff).max(0) as usize;
                let new_end = ((token.end_byte as isize + diff).max(0) as usize).max(new_start);
                if new_end > new_start {
                    token.start_byte = new_start;
                    token.end_byte = new_end;
                    true
                } else {
                    false
                }
            } else {
                // Overlapping edit: drop token
                false
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shift_tokens_insert_before() {
        let tokens = vec![
            HighlightToken::new(10, 15, TokenType::Keyword),
            HighlightToken::new(20, 25, TokenType::Type),
        ];
        // Insert 5 bytes at offset 0 (0 -> 5 bytes)
        let deltas = vec![ByteDelta::new(0, 0, 5)];
        let shifted = shift_tokens_for_deltas(&tokens, &deltas);

        assert_eq!(shifted.len(), 2);
        assert_eq!(shifted[0].start_byte, 15);
        assert_eq!(shifted[0].end_byte, 20);
        assert_eq!(shifted[1].start_byte, 25);
        assert_eq!(shifted[1].end_byte, 30);
    }

    #[test]
    fn test_shift_tokens_delete_before() {
        let tokens = vec![
            HighlightToken::new(10, 15, TokenType::Keyword),
            HighlightToken::new(20, 25, TokenType::Type),
        ];
        // Delete 4 bytes at offset 0 (4 -> 0 bytes)
        let deltas = vec![ByteDelta::new(0, 4, 0)];
        let shifted = shift_tokens_for_deltas(&tokens, &deltas);

        assert_eq!(shifted.len(), 2);
        assert_eq!(shifted[0].start_byte, 6);
        assert_eq!(shifted[0].end_byte, 11);
        assert_eq!(shifted[1].start_byte, 16);
        assert_eq!(shifted[1].end_byte, 21);
    }

    #[test]
    fn test_shift_tokens_overlap_drops_token() {
        let tokens = vec![
            HighlightToken::new(0, 5, TokenType::Comment),
            HighlightToken::new(10, 15, TokenType::Keyword),
            HighlightToken::new(20, 25, TokenType::Type),
        ];
        // Edit inside token 1 (offset 12..14)
        let deltas = vec![ByteDelta::new(12, 2, 4)];
        let shifted = shift_tokens_for_deltas(&tokens, &deltas);

        assert_eq!(shifted.len(), 2);
        // Token 0 strictly before edit remains unchanged
        assert_eq!(shifted[0].start_byte, 0);
        assert_eq!(shifted[0].end_byte, 5);
        // Token 1 overlapping is dropped
        // Token 2 strictly after edit shifts by +2
        assert_eq!(shifted[1].start_byte, 22);
        assert_eq!(shifted[1].end_byte, 27);
    }

    #[test]
    fn test_shift_tokens_multibyte_utf8() {
        let tokens = vec![HighlightToken::new(5, 10, TokenType::String)];
        // Insert a 4-byte emoji (e.g. 🦀) at offset 2 (0 -> 4 bytes)
        let deltas = vec![ByteDelta::new(2, 0, 4)];
        let shifted = shift_tokens_for_deltas(&tokens, &deltas);

        assert_eq!(shifted.len(), 1);
        assert_eq!(shifted[0].start_byte, 9);
        assert_eq!(shifted[0].end_byte, 14);
    }

    #[test]
    fn test_shift_tokens_multiple_deltas() {
        let tokens = vec![HighlightToken::new(20, 30, TokenType::Function)];
        let deltas = vec![
            ByteDelta::new(0, 0, 5),  // shift +5 => 25..35
            ByteDelta::new(10, 2, 0), // delete 2 => shift -2 => 23..33
        ];
        let shifted = shift_tokens_for_deltas(&tokens, &deltas);

        assert_eq!(shifted.len(), 1);
        assert_eq!(shifted[0].start_byte, 23);
        assert_eq!(shifted[0].end_byte, 33);
    }
}
