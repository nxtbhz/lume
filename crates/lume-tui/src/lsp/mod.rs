pub mod protocol;
pub mod session;

use std::fmt;
use std::path::PathBuf;

/// Supported language servers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ServerKind {
    Clangd,
}

impl ServerKind {
    pub fn executable_name(self) -> &'static str {
        match self {
            Self::Clangd => "clangd",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Clangd => "clangd",
        }
    }
}

/// LSP 3.17 position encoding negotiated during `initialize`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PositionEncoding {
    Utf8,
    Utf16,
    Utf32,
}

/// Zero-based line and character offset in an LSP document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

/// Resolved local file target returned by `textDocument/definition`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DefinitionTarget {
    pub path: PathBuf,
    pub encoding: PositionEncoding,
    pub position: Position,
}

/// Errors produced by the LSP client, session, or protocol layers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LspError {
    MissingExecutable(ServerKind),
    Timeout,
    Exited,
    Unsupported,
    ContentModified,
    Project(String),
    Protocol(String),
}

impl fmt::Display for LspError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingExecutable(kind) => {
                write!(
                    f,
                    "Language server '{}' not found in PATH",
                    kind.display_name()
                )
            }
            Self::Timeout => write!(f, "Language server request timed out"),
            Self::Exited => write!(f, "Language server exited unexpectedly"),
            Self::Unsupported => write!(f, "Go to Definition is not supported for this file"),
            Self::ContentModified => {
                write!(f, "Document changed during definition lookup; please retry")
            }
            Self::Project(msg) => write!(f, "Project error: {msg}"),
            Self::Protocol(msg) => write!(f, "LSP protocol error: {msg}"),
        }
    }
}

impl std::error::Error for LspError {}
