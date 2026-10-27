pub mod highlighter;
pub mod language;
mod lexer;
pub mod token;
pub mod worker;

pub use highlighter::SyntaxHighlighter;
pub use language::Language;
pub use token::{HighlightToken, TokenType};
pub use worker::{SyntaxRequest, SyntaxResponse, SyntaxWorker};

#[cfg(test)]
mod tests;
