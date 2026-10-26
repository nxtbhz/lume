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
