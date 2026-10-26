pub mod highlighter;
pub mod language;
pub mod token;
pub mod worker;

pub use highlighter::SyntaxHighlighter;
pub use language::Language;
pub use token::{HighlightToken, TokenType};
pub use worker::{SyntaxRequest, SyntaxResponse, SyntaxWorker};

#[cfg(test)]
mod tests;
