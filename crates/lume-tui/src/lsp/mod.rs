pub mod documents;
pub mod project;
pub mod protocol;
pub mod session;

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::thread::JoinHandle;

use crossbeam_channel::{Receiver, Sender};
use lume_core::RopeBuffer;
use lume_syntax::Language;

use crate::lsp::documents::DocumentSyncState;
use crate::lsp::project::{canonicalize_best_effort, project_for, server_for, ProjectKey};
use crate::lsp::session::LspSession;

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

/// Snapshot of an open editor tab passed to the LSP worker.
#[derive(Clone, Debug)]
pub struct OpenDocument {
    pub path: PathBuf,
    pub language: Language,
    pub tab_id: usize,
    pub revision: u64,
    pub text: RopeBuffer,
}

/// Definition query dispatched off the TUI thread.
#[derive(Clone, Debug)]
pub struct DefinitionQuery {
    pub id: u64,
    pub source_path: PathBuf,
    pub source_byte: usize,
    pub explorer_root: PathBuf,
    pub documents: Vec<OpenDocument>,
}

/// Definition response delivered back to the TUI event loop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefinitionReply {
    pub id: u64,
    pub result: Result<Vec<DefinitionTarget>, LspError>,
}

type DeliverFn = Box<dyn Fn(DefinitionReply) + Send + 'static>;
type SessionFactory = Box<dyn Fn(&ProjectKey) -> Result<LspSession, LspError> + Send + 'static>;

enum ClientMode {
    Lazy {
        deliver: Option<DeliverFn>,
        factory: Option<SessionFactory>,
        sender: Option<Sender<DefinitionQuery>>,
        worker: Option<JoinHandle<()>>,
    },
    #[cfg(test)]
    TestSender(Sender<DefinitionQuery>),
}

/// Lazily-started background LSP client that manages one session per `(ServerKind, root)`.
pub struct LspClient {
    mode: ClientMode,
}

impl Drop for LspClient {
    fn drop(&mut self) {
        if let ClientMode::Lazy { sender, worker, .. } = &mut self.mode {
            // Closing the channel signals the worker loop to exit and shut down all sessions.
            sender.take();
            if let Some(handle) = worker.take() {
                let _ = handle.join();
            }
        }
    }
}

impl LspClient {
    /// Registers the reply delivery callback without starting any background thread or server.
    pub fn new(deliver: impl Fn(DefinitionReply) + Send + 'static) -> Self {
        Self {
            mode: ClientMode::Lazy {
                deliver: Some(Box::new(deliver)),
                factory: Some(Box::new(|key: &ProjectKey| {
                    LspSession::spawn(key.server, &key.root)
                })),
                sender: None,
                worker: None,
            },
        }
    }

    #[cfg(test)]
    pub fn with_test_sender(sender: Sender<DefinitionQuery>) -> Self {
        Self {
            mode: ClientMode::TestSender(sender),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_session_factory(
        deliver: impl Fn(DefinitionReply) + Send + 'static,
        factory: impl Fn(&ProjectKey) -> Result<LspSession, LspError> + Send + 'static,
    ) -> Self {
        Self {
            mode: ClientMode::Lazy {
                deliver: Some(Box::new(deliver)),
                factory: Some(Box::new(factory)),
                sender: None,
                worker: None,
            },
        }
    }

    /// Dispatches a definition query to the background LSP worker (spawning it on first use).
    pub fn request_definition(&mut self, query: DefinitionQuery) -> Result<(), LspError> {
        match &mut self.mode {
            #[cfg(test)]
            ClientMode::TestSender(tx) => tx.send(query).map_err(|_| LspError::Exited),
            ClientMode::Lazy {
                deliver,
                factory,
                sender,
                worker,
            } => {
                if sender.is_none() {
                    let deliver_fn = deliver.take().ok_or(LspError::Exited)?;
                    let factory_fn = factory.take().ok_or(LspError::Exited)?;
                    let (tx, rx) = crossbeam_channel::unbounded();
                    let handle = std::thread::Builder::new()
                        .name("lume-lsp-worker".to_string())
                        .spawn(move || run_worker(rx, deliver_fn, factory_fn))
                        .map_err(|e| {
                            LspError::Protocol(format!("failed to start LSP worker thread: {e}"))
                        })?;
                    *sender = Some(tx);
                    *worker = Some(handle);
                }
                sender
                    .as_ref()
                    .ok_or(LspError::Exited)?
                    .send(query)
                    .map_err(|_| LspError::Exited)
            }
        }
    }
}

struct SessionEntry {
    session: LspSession,
    docs: DocumentSyncState,
}

fn run_worker(rx: Receiver<DefinitionQuery>, deliver: DeliverFn, factory: SessionFactory) {
    let mut sessions: HashMap<ProjectKey, SessionEntry> = HashMap::new();

    while let Ok(query) = rx.recv() {
        let query_id = query.id;
        let result = handle_query(&mut sessions, &factory, &query);
        deliver(DefinitionReply {
            id: query_id,
            result,
        });
    }
    // Dropping `sessions` gracefully shuts down and reaps every active server process.
}

fn handle_query(
    sessions: &mut HashMap<ProjectKey, SessionEntry>,
    factory: &SessionFactory,
    query: &DefinitionQuery,
) -> Result<Vec<DefinitionTarget>, LspError> {
    let source_canonical = canonicalize_best_effort(&query.source_path, &query.explorer_root);
    let source_doc = query
        .documents
        .iter()
        .find(|doc| canonicalize_best_effort(&doc.path, &query.explorer_root) == source_canonical)
        .ok_or_else(|| {
            LspError::Project(format!(
                "source document '{}' missing from open documents",
                query.source_path.display()
            ))
        })?;

    let (server_kind, language_id) =
        server_for(&source_doc.path, source_doc.language).ok_or(LspError::Unsupported)?;
    let project_key = project_for(&source_canonical, server_kind, &query.explorer_root)?;

    // Drop any dead session so it is recreated cleanly with fresh document sync state.
    if sessions
        .get(&project_key)
        .is_some_and(|entry| !entry.session.is_alive())
    {
        sessions.remove(&project_key);
    }

    if !sessions.contains_key(&project_key) {
        let session = factory(&project_key)?;
        sessions.insert(
            project_key.clone(),
            SessionEntry {
                session,
                docs: DocumentSyncState::new(),
            },
        );
    }

    let entry = sessions.get_mut(&project_key).ok_or(LspError::Exited)?;
    let result = entry.docs.query_definition(
        &mut entry.session,
        &project_key,
        query,
        language_id,
        &source_canonical,
    );

    if !entry.session.is_alive() {
        sessions.remove(&project_key);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::project::ProjectKey;
    use crate::lsp::session::{
        IncomingMessage, LspSession, ProcessHandle, SessionClock, WaitError,
    };
    use crate::tests::TestDir;
    use crossbeam_channel::Receiver;
    use serde_json::json;
    use std::io::{self, Write};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct SinkWriter;
    impl Write for SinkWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct NoopProcess;
    impl ProcessHandle for NoopProcess {
        fn terminate_and_reap(&mut self) {}
    }

    struct InstantClock;
    impl SessionClock for InstantClock {
        fn now(&self) -> Duration {
            Duration::from_millis(0)
        }
        fn recv_until(
            &self,
            rx: &Receiver<IncomingMessage>,
            _deadline: Duration,
        ) -> Result<IncomingMessage, WaitError> {
            rx.recv().map_err(|_| WaitError::Disconnected)
        }
    }

    #[test]
    fn routes_distinct_c_projects_to_separate_sessions() {
        let dir = TestDir::new("lume_lsp_two_c_projects");
        let proj_a = dir.path().join("proj_a");
        let proj_b = dir.path().join("proj_b");
        std::fs::create_dir_all(&proj_a).unwrap();
        std::fs::create_dir_all(&proj_b).unwrap();
        std::fs::write(proj_a.join("compile_flags.txt"), "-Iinclude\n").unwrap();
        std::fs::write(proj_b.join("compile_flags.txt"), "-Iinclude\n").unwrap();

        let file_a = proj_a.join("a.c");
        let file_b = proj_b.join("b.cpp");
        std::fs::write(&file_a, "int a = 1;\n").unwrap();
        std::fs::write(&file_b, "int b = 2;\n").unwrap();

        let spawned_keys: Arc<Mutex<Vec<ProjectKey>>> = Arc::new(Mutex::new(Vec::new()));
        let spawned_clone = Arc::clone(&spawned_keys);

        let (reply_tx, reply_rx) = crossbeam_channel::unbounded();
        let mut client = LspClient::with_session_factory(
            move |reply| {
                let _ = reply_tx.send(reply);
            },
            move |key: &ProjectKey| {
                spawned_clone.lock().unwrap().push(key.clone());
                let (tx, rx) = crossbeam_channel::unbounded();
                tx.send(IncomingMessage::Message(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": {
                        "capabilities": {
                            "positionEncoding": "utf-8",
                            "definitionProvider": true,
                            "textDocumentSync": 2
                        }
                    }
                })))
                .unwrap();
                let target_uri =
                    crate::lsp::protocol::path_to_uri(&key.root.join("target.h")).unwrap();
                tx.send(IncomingMessage::Message(json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "uri": target_uri,
                        "range": {
                            "start": { "line": 0, "character": 0 },
                            "end": { "line": 0, "character": 1 }
                        }
                    }
                })))
                .unwrap();
                // Allow clean shutdown reply
                tx.send(IncomingMessage::Message(json!({
                    "jsonrpc": "2.0",
                    "id": 3,
                    "result": null
                })))
                .unwrap();
                LspSession::from_parts(
                    Box::new(SinkWriter),
                    rx,
                    Box::new(NoopProcess),
                    Box::new(InstantClock),
                    &key.root,
                )
            },
        );

        client
            .request_definition(DefinitionQuery {
                id: 10,
                source_path: file_a.clone(),
                source_byte: 4,
                explorer_root: dir.path().to_path_buf(),
                documents: vec![OpenDocument {
                    path: file_a,
                    language: Language::C,
                    tab_id: 1,
                    revision: 0,
                    text: RopeBuffer::from_str("int a = 1;\n"),
                }],
            })
            .unwrap();

        client
            .request_definition(DefinitionQuery {
                id: 11,
                source_path: file_b.clone(),
                source_byte: 4,
                explorer_root: dir.path().to_path_buf(),
                documents: vec![OpenDocument {
                    path: file_b,
                    language: Language::Cpp,
                    tab_id: 2,
                    revision: 0,
                    text: RopeBuffer::from_str("int b = 2;\n"),
                }],
            })
            .unwrap();

        let r1 = reply_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let r2 = reply_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(r1.id, 10);
        assert_eq!(r2.id, 11);
        assert_eq!(r1.result.unwrap()[0].path, proj_a.join("target.h"));
        assert_eq!(r2.result.unwrap()[0].path, proj_b.join("target.h"));
        assert_eq!(spawned_keys.lock().unwrap().len(), 2);
    }
}
