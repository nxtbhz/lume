//! Configuration parsing, keymap resolution, and settings schema for Lume.

pub mod keymap;
pub mod toml_parser;

use std::path::PathBuf;

/// Errors produced while reading, parsing, or validating `config.toml`.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Failed to read or create the configuration file on disk.
    #[error("failed to access config `{}`: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Syntax error encountered while parsing TOML content.
    #[error("config parse error on line {line}: {message}")]
    Parse { line: usize, message: String },
    /// Semantic error (unknown section/key, out-of-range value, invalid keybinding or action).
    #[error("{}", format_validation_error(*line, message))]
    Validation {
        line: Option<usize>,
        message: String,
    },
}

fn format_validation_error(line: Option<usize>, message: &str) -> String {
    match line {
        Some(line_no) => format!("config validation error on line {line_no}: {message}"),
        None => format!("config validation error: {message}"),
    }
}
