pub mod highlighter;
pub mod language;
mod lexer;
pub mod token;
pub mod worker;

pub use highlighter::SyntaxHighlighter;
pub use language::Language;
pub use token::{
    shift_tokens_for_deltas, shift_tokens_in_place, ByteDelta, HighlightToken, TokenType,
};
pub use worker::{SyntaxCommand, SyntaxRequest, SyntaxResponse, SyntaxWorker};

#[cfg(test)]
mod tests;
