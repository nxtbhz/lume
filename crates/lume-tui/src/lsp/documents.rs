use std::collections::BTreeMap;
use std::path::Path;

use lume_core::{RopeBuffer, TextBuffer};
use serde_json::json;

use crate::lsp::project::{canonicalize_best_effort, project_for, server_for, ProjectKey};
use crate::lsp::protocol::{parse_definition, path_to_uri, position_at_byte};
use crate::lsp::session::{LspSession, TextDocumentSyncKind};
use crate::lsp::{DefinitionQuery, DefinitionTarget, LspError, OpenDocument};

struct SyncedDocument {
    tab_id: usize,
    revision: u64,
    language_id: &'static str,
    lsp_version: i32,
    last_text: RopeBuffer,
    last_sync_epoch: u64,
    preamble_epoch: u64,
}

/// Per-session synchronization ledger tracking open URIs, revisions, and last sent snapshots.
#[derive(Default)]
pub struct DocumentSyncState {
    synced: BTreeMap<String, SyncedDocument>,
    sync_epoch: u64,
    last_closed_epoch: u64,
}

impl DocumentSyncState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reconciles eligible open documents for `project` onto `session` before sending
    /// `textDocument/definition` for `query`.
    pub fn query_definition(
        &mut self,
        session: &mut LspSession,
        project: &ProjectKey,
        query: &DefinitionQuery,
        source_language_id: &'static str,
        source_canonical: &Path,
    ) -> Result<Vec<DefinitionTarget>, LspError> {
        let caps = session.capabilities();
        let source_uri = path_to_uri(source_canonical)?;

        let mut desired: BTreeMap<String, (&OpenDocument, &'static str)> = BTreeMap::new();
        let mut source_doc_ref: Option<&OpenDocument> = None;

        for doc in &query.documents {
            let doc_canon = canonicalize_best_effort(&doc.path, &query.explorer_root);
            if doc_canon == source_canonical {
                source_doc_ref = Some(doc);
                desired.insert(source_uri.clone(), (doc, source_language_id));
                continue;
            }
            let Some((kind, lang_id)) = server_for(&doc.path, doc.language) else {
                continue;
            };
            if kind != project.server {
                continue;
            }
            let Ok(doc_project) = project_for(&doc_canon, kind, &query.explorer_root) else {
                continue;
            };
            if doc_project != *project {
                continue;
            }
            if let Ok(uri) = path_to_uri(&doc_canon) {
                desired.insert(uri, (doc, lang_id));
            }
        }

        let source_doc = source_doc_ref.ok_or_else(|| {
            LspError::Project(format!(
                "source document '{}' missing from query snapshot",
                query.source_path.display()
            ))
        })?;

        // 1. Close URIs that were closed, renamed, moved to another project, or changed languageId.
        let to_close: Vec<String> = self
            .synced
            .iter()
            .filter_map(|(uri, tracked)| match desired.get(uri) {
                None => Some(uri.clone()),
                Some((_, lang_id)) if *lang_id != tracked.language_id => Some(uri.clone()),
                Some(_) => None,
            })
            .collect();

        for uri in to_close {
            self.synced.remove(&uri);
            self.sync_epoch = self.sync_epoch.saturating_add(1);
            self.last_closed_epoch = self.sync_epoch;
            if caps.open_close {
                session.notify(
                    "textDocument/didClose",
                    json!({ "textDocument": { "uri": uri } }),
                )?;
            }
        }

        // 2. Sync all non-source project documents first so clangd's dirty-header overlay
        //    has updated header contents before the source TU is opened or refreshed.
        let source_entry = desired.remove(&source_uri);
        for (uri, (doc, language_id)) in desired {
            let _ = self.sync_one(session, caps, uri, doc, language_id, false)?;
        }

        // 3. Sync the source document last. If any other project document (e.g. a header)
        //    was opened, changed, or closed after the source TU's preamble was last opened—
        //    even during an earlier query from another file—force a didClose + didOpen cycle
        //    so clangd rebuilds the source TU's preamble synchronously against dirty headers.
        if let Some((doc, language_id)) = source_entry {
            let force_reopen = self.synced.get(&source_uri).is_some_and(|source| {
                self.last_closed_epoch > source.preamble_epoch
                    || self.synced.iter().any(|(uri, other)| {
                        uri != &source_uri && other.last_sync_epoch > source.preamble_epoch
                    })
            });
            let _ = self.sync_one(
                session,
                caps,
                source_uri.clone(),
                doc,
                language_id,
                force_reopen,
            )?;
        }

        // 3. Issue textDocument/definition on the same session after sync notifications.
        if !caps.definition_provider {
            return Err(LspError::Unsupported);
        }

        let source_pos =
            position_at_byte(&source_doc.text, query.source_byte, caps.position_encoding)
                .ok_or_else(|| {
                    LspError::Protocol(format!(
                        "cursor byte {} is not a valid LSP position",
                        query.source_byte
                    ))
                })?;

        let raw_result = session.request(
            "textDocument/definition",
            json!({
                "textDocument": { "uri": source_uri },
                "position": {
                    "line": source_pos.line,
                    "character": source_pos.character,
                }
            }),
        )?;

        parse_definition(raw_result, caps.position_encoding)
    }

    fn sync_one(
        &mut self,
        session: &mut LspSession,
        caps: crate::lsp::session::ServerCapabilities,
        uri: String,
        doc: &OpenDocument,
        language_id: &'static str,
        force_reopen: bool,
    ) -> Result<bool, LspError> {
        if force_reopen {
            if let Some(tracked) = self.synced.get_mut(&uri) {
                let full_text = doc.text.rope().to_string();
                if caps.open_close {
                    session.notify(
                        "textDocument/didClose",
                        json!({ "textDocument": { "uri": uri } }),
                    )?;
                    tracked.lsp_version = 1;
                    session.notify(
                        "textDocument/didOpen",
                        json!({
                            "textDocument": {
                                "uri": uri,
                                "languageId": language_id,
                                "version": 1,
                                "text": full_text,
                            }
                        }),
                    )?;
                }
                self.sync_epoch = self.sync_epoch.saturating_add(1);
                tracked.tab_id = doc.tab_id;
                tracked.revision = doc.revision;
                tracked.last_text = doc.text.clone();
                tracked.last_sync_epoch = self.sync_epoch;
                tracked.preamble_epoch = self.sync_epoch;
                return Ok(true);
            }
        }

        match self.synced.get_mut(&uri) {
            None => {
                let full_text = doc.text.rope().to_string();
                if caps.open_close {
                    session.notify(
                        "textDocument/didOpen",
                        json!({
                            "textDocument": {
                                "uri": uri,
                                "languageId": language_id,
                                "version": 1,
                                "text": full_text,
                            }
                        }),
                    )?;
                }
                self.sync_epoch = self.sync_epoch.saturating_add(1);
                self.synced.insert(
                    uri,
                    SyncedDocument {
                        tab_id: doc.tab_id,
                        revision: doc.revision,
                        language_id,
                        lsp_version: 1,
                        last_text: doc.text.clone(),
                        last_sync_epoch: self.sync_epoch,
                        preamble_epoch: self.sync_epoch,
                    },
                );
                Ok(true)
            }
            Some(tracked) => {
                if tracked.tab_id == doc.tab_id && tracked.revision == doc.revision {
                    return Ok(false);
                }
                let full_text = doc.text.rope().to_string();
                let mut reopened = false;
                match caps.sync_kind {
                    TextDocumentSyncKind::Full => {
                        tracked.lsp_version = tracked.lsp_version.saturating_add(1);
                        session.notify(
                            "textDocument/didChange",
                            json!({
                                "textDocument": {
                                    "uri": uri,
                                    "version": tracked.lsp_version,
                                },
                                "contentChanges": [{
                                    "text": full_text,
                                }]
                            }),
                        )?;
                    }
                    TextDocumentSyncKind::Incremental => {
                        let end_pos = position_at_byte(
                            &tracked.last_text,
                            tracked.last_text.len_bytes(),
                            caps.position_encoding,
                        )
                        .ok_or_else(|| {
                            LspError::Protocol(
                                "failed to compute previous snapshot end position".to_string(),
                            )
                        })?;
                        tracked.lsp_version = tracked.lsp_version.saturating_add(1);
                        session.notify(
                            "textDocument/didChange",
                            json!({
                                "textDocument": {
                                    "uri": uri,
                                    "version": tracked.lsp_version,
                                },
                                "contentChanges": [{
                                    "range": {
                                        "start": { "line": 0, "character": 0 },
                                        "end": { "line": end_pos.line, "character": end_pos.character }
                                    },
                                    "text": full_text,
                                }]
                            }),
                        )?;
                    }
                    TextDocumentSyncKind::None => {
                        if caps.open_close {
                            session.notify(
                                "textDocument/didClose",
                                json!({ "textDocument": { "uri": uri } }),
                            )?;
                            tracked.lsp_version = 1;
                            session.notify(
                                "textDocument/didOpen",
                                json!({
                                    "textDocument": {
                                        "uri": uri,
                                        "languageId": language_id,
                                        "version": 1,
                                        "text": full_text,
                                    }
                                }),
                            )?;
                            reopened = true;
                        }
                    }
                }
                self.sync_epoch = self.sync_epoch.saturating_add(1);
                tracked.tab_id = doc.tab_id;
                tracked.revision = doc.revision;
                tracked.last_text = doc.text.clone();
                tracked.last_sync_epoch = self.sync_epoch;
                if reopened {
                    tracked.preamble_epoch = self.sync_epoch;
                }
                Ok(true)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::protocol::{path_to_uri, read_message};
    use crate::lsp::session::{IncomingMessage, ProcessHandle, SessionClock, WaitError};
    use crate::lsp::{OpenDocument, ServerKind};
    use crate::tests::TestDir;
    use crossbeam_channel::Receiver;
    use lume_core::RopeBuffer;
    use lume_syntax::Language;
    use serde_json::{json, Value};
    use std::io::{self, Cursor, Write};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct SharedWriter(Arc<Mutex<Vec<u8>>>);
    impl Write for SharedWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
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
            _cancellable: bool,
        ) -> Result<IncomingMessage, WaitError> {
            rx.recv().map_err(|_| WaitError::Disconnected)
        }
    }

    fn take_sent_messages(buf: &Arc<Mutex<Vec<u8>>>) -> Vec<Value> {
        let mut guard = buf.lock().unwrap();
        let bytes = std::mem::take(&mut *guard);
        let mut cursor = Cursor::new(bytes);
        let mut out = Vec::new();
        while let Some(msg) = read_message(&mut cursor).unwrap() {
            out.push(msg);
        }
        out
    }

    fn make_session(
        root: &Path,
        sync_kind: u64,
        encoding: &str,
    ) -> (
        LspSession,
        crossbeam_channel::Sender<IncomingMessage>,
        Arc<Mutex<Vec<u8>>>,
    ) {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = crossbeam_channel::unbounded();
        tx.send(IncomingMessage::Message(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "capabilities": {
                    "positionEncoding": encoding,
                    "definitionProvider": true,
                    "textDocumentSync": { "openClose": true, "change": sync_kind }
                }
            }
        })))
        .unwrap();
        let session = LspSession::from_parts(
            Box::new(SharedWriter(Arc::clone(&sent))),
            rx,
            Box::new(NoopProcess),
            Box::new(InstantClock),
            root,
        )
        .unwrap();
        let _ = take_sent_messages(&sent);
        (session, tx, sent)
    }

    #[test]
    fn request_flushes_unsaved_documents() {
        let dir = TestDir::new("lume_lsp_flush_unsaved");
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::create_dir_all(dir.path().join("include/arq/aaa")).unwrap();
        std::fs::write(dir.path().join("compile_flags.txt"), "-Iinclude\n").unwrap();

        let source_path = dir.path().join("src/t.c");
        let header_path = dir.path().join("include/arq/aaa/arch.h");
        let source_text = "#include \"arq/aaa/arch.h\"\nstruct Arch item;\n";
        let header_v0 = "struct Arch { int v; };\n";
        let header_v1 = "\n\nstruct Arch { int v; };\n";
        std::fs::write(&source_path, source_text).unwrap();
        std::fs::write(&header_path, header_v0).unwrap();

        let source_uri = path_to_uri(&source_path).unwrap();
        let header_uri = path_to_uri(&header_path).unwrap();
        let project = ProjectKey {
            server: ServerKind::Clangd,
            root: dir.path().to_path_buf(),
        };

        let (mut session, tx, sent) = make_session(dir.path(), 2, "utf-16");
        let mut sync = DocumentSyncState::new();

        // First query opens both documents before definition
        tx.send(IncomingMessage::Message(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": null
        })))
        .unwrap();
        let q1 = DefinitionQuery {
            id: 1,
            source_path: source_path.clone(),
            source_byte: source_text.find("Arch").unwrap(),
            explorer_root: dir.path().to_path_buf(),
            documents: vec![
                OpenDocument {
                    path: source_path.clone(),
                    language: Language::C,
                    tab_id: 1,
                    revision: 0,
                    text: RopeBuffer::from_str(source_text),
                },
                OpenDocument {
                    path: header_path.clone(),
                    language: Language::C,
                    tab_id: 2,
                    revision: 0,
                    text: RopeBuffer::from_str(header_v0),
                },
            ],
        };
        sync.query_definition(&mut session, &project, &q1, "c", &source_path)
            .unwrap();
        let initial_msgs = take_sent_messages(&sent);
        assert_eq!(initial_msgs.len(), 3);
        assert_eq!(initial_msgs[0]["method"], "textDocument/didOpen");
        assert_eq!(initial_msgs[1]["method"], "textDocument/didOpen");
        assert_eq!(initial_msgs[2]["method"], "textDocument/definition");

        // Edit ONLY the header in memory (revision 1), then query from src/t.c again
        tx.send(IncomingMessage::Message(json!({
            "jsonrpc": "2.0",
            "id": 3,
            "result": null
        })))
        .unwrap();
        let q2 = DefinitionQuery {
            id: 2,
            source_path: source_path.clone(),
            source_byte: source_text.find("Arch").unwrap(),
            explorer_root: dir.path().to_path_buf(),
            documents: vec![
                OpenDocument {
                    path: source_path.clone(),
                    language: Language::C,
                    tab_id: 1,
                    revision: 0,
                    text: RopeBuffer::from_str(source_text),
                },
                OpenDocument {
                    path: header_path.clone(),
                    language: Language::C,
                    tab_id: 2,
                    revision: 1,
                    text: RopeBuffer::from_str(header_v1),
                },
            ],
        };
        sync.query_definition(&mut session, &project, &q2, "c", &source_path)
            .unwrap();
        let messages = take_sent_messages(&sent);
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0]["method"], "textDocument/didChange");
        assert_eq!(messages[0]["params"]["textDocument"]["uri"], header_uri);
        assert_eq!(messages[1]["method"], "textDocument/didClose");
        assert_eq!(messages[1]["params"]["textDocument"]["uri"], source_uri);
        assert_eq!(messages[2]["method"], "textDocument/didOpen");
        assert_eq!(messages[2]["params"]["textDocument"]["uri"], source_uri);
        assert_eq!(messages[3]["method"], "textDocument/definition");
        assert_eq!(messages[3]["params"]["textDocument"]["uri"], source_uri);
    }

    #[test]
    fn supports_sync_kinds_and_closes_renamed_uris() {
        let dir = TestDir::new("lume_lsp_sync_kinds");
        std::fs::write(dir.path().join("compile_flags.txt"), "-I.\n").unwrap();
        let old_path = dir.path().join("old.c");
        let new_path = dir.path().join("new.c");
        std::fs::write(&old_path, "int x;\n").unwrap();
        std::fs::write(&new_path, "int x;\n").unwrap();
        let old_uri = path_to_uri(&old_path).unwrap();
        let new_uri = path_to_uri(&new_path).unwrap();
        let project = ProjectKey {
            server: ServerKind::Clangd,
            root: dir.path().to_path_buf(),
        };

        // 1. Full sync (change = 1): didChange omits `range`, skips unchanged revisions, sends didClose on rename
        let (mut full_session, full_tx, full_sent) = make_session(dir.path(), 1, "utf-16");
        let mut full_sync = DocumentSyncState::new();

        full_tx
            .send(IncomingMessage::Message(
                json!({ "jsonrpc": "2.0", "id": 2, "result": null }),
            ))
            .unwrap();
        let q1 = DefinitionQuery {
            id: 1,
            source_path: old_path.clone(),
            source_byte: 4,
            explorer_root: dir.path().to_path_buf(),
            documents: vec![OpenDocument {
                path: old_path.clone(),
                language: Language::C,
                tab_id: 1,
                revision: 0,
                text: RopeBuffer::from_str("int x;\n"),
            }],
        };
        full_sync
            .query_definition(&mut full_session, &project, &q1, "c", &old_path)
            .unwrap();
        let _ = take_sent_messages(&full_sent);

        // Same revision -> no duplicate didChange
        full_tx
            .send(IncomingMessage::Message(
                json!({ "jsonrpc": "2.0", "id": 3, "result": null }),
            ))
            .unwrap();
        full_sync
            .query_definition(&mut full_session, &project, &q1, "c", &old_path)
            .unwrap();
        let same_rev_msgs = take_sent_messages(&full_sent);
        assert_eq!(same_rev_msgs.len(), 1);
        assert_eq!(same_rev_msgs[0]["method"], "textDocument/definition");

        // Updated revision in Full mode -> no `range` key
        full_tx
            .send(IncomingMessage::Message(
                json!({ "jsonrpc": "2.0", "id": 4, "result": null }),
            ))
            .unwrap();
        let q2 = DefinitionQuery {
            id: 2,
            source_path: old_path.clone(),
            source_byte: 4,
            explorer_root: dir.path().to_path_buf(),
            documents: vec![OpenDocument {
                path: old_path.clone(),
                language: Language::C,
                tab_id: 1,
                revision: 1,
                text: RopeBuffer::from_str("int y = 2;\n"),
            }],
        };
        full_sync
            .query_definition(&mut full_session, &project, &q2, "c", &old_path)
            .unwrap();
        let full_change_msgs = take_sent_messages(&full_sent);
        assert_eq!(full_change_msgs[0]["method"], "textDocument/didChange");
        assert!(full_change_msgs[0]["params"]["contentChanges"][0]
            .get("range")
            .is_none());
        assert_eq!(
            full_change_msgs[0]["params"]["contentChanges"][0]["text"],
            "int y = 2;\n"
        );

        // Rename old.c -> new.c -> didClose(old_uri), didOpen(new_uri), definition(new_uri)
        full_tx
            .send(IncomingMessage::Message(
                json!({ "jsonrpc": "2.0", "id": 5, "result": null }),
            ))
            .unwrap();
        let q3 = DefinitionQuery {
            id: 3,
            source_path: new_path.clone(),
            source_byte: 4,
            explorer_root: dir.path().to_path_buf(),
            documents: vec![OpenDocument {
                path: new_path.clone(),
                language: Language::C,
                tab_id: 1,
                revision: 1,
                text: RopeBuffer::from_str("int y = 2;\n"),
            }],
        };
        full_sync
            .query_definition(&mut full_session, &project, &q3, "c", &new_path)
            .unwrap();
        let rename_msgs = take_sent_messages(&full_sent);
        assert_eq!(rename_msgs[0]["method"], "textDocument/didClose");
        assert_eq!(rename_msgs[0]["params"]["textDocument"]["uri"], old_uri);
        assert_eq!(rename_msgs[1]["method"], "textDocument/didOpen");
        assert_eq!(rename_msgs[1]["params"]["textDocument"]["uri"], new_uri);
        assert_eq!(rename_msgs[2]["method"], "textDocument/definition");

        // 2. Incremental sync (change = 2) with UTF-16 encoding: range covers exact previous snapshot
        let (mut inc_session, inc_tx, inc_sent) = make_session(dir.path(), 2, "utf-16");
        let mut inc_sync = DocumentSyncState::new();
        let prev_text = "// 😀\r\nint x;"; // line 0: "// 😀" -> end at line 1, char 6 (UTF-16)
        inc_tx
            .send(IncomingMessage::Message(
                json!({ "jsonrpc": "2.0", "id": 2, "result": null }),
            ))
            .unwrap();
        let iq1 = DefinitionQuery {
            id: 10,
            source_path: old_path.clone(),
            source_byte: 0,
            explorer_root: dir.path().to_path_buf(),
            documents: vec![OpenDocument {
                path: old_path.clone(),
                language: Language::C,
                tab_id: 1,
                revision: 0,
                text: RopeBuffer::from_str(prev_text),
            }],
        };
        inc_sync
            .query_definition(&mut inc_session, &project, &iq1, "c", &old_path)
            .unwrap();
        let _ = take_sent_messages(&inc_sent);

        inc_tx
            .send(IncomingMessage::Message(
                json!({ "jsonrpc": "2.0", "id": 3, "result": null }),
            ))
            .unwrap();
        let iq2 = DefinitionQuery {
            id: 11,
            source_path: old_path.clone(),
            source_byte: 0,
            explorer_root: dir.path().to_path_buf(),
            documents: vec![OpenDocument {
                path: old_path.clone(),
                language: Language::C,
                tab_id: 1,
                revision: 1,
                text: RopeBuffer::from_str("int z;"),
            }],
        };
        inc_sync
            .query_definition(&mut inc_session, &project, &iq2, "c", &old_path)
            .unwrap();
        let inc_msgs = take_sent_messages(&inc_sent);
        assert_eq!(inc_msgs[0]["method"], "textDocument/didChange");
        let change = &inc_msgs[0]["params"]["contentChanges"][0];
        assert_eq!(
            change["range"]["start"],
            json!({ "line": 0, "character": 0 })
        );
        assert_eq!(change["range"]["end"], json!({ "line": 1, "character": 6 }));
        assert_eq!(change["text"], "int z;");
    }

    #[test]
    fn reopens_source_preamble_after_intermediate_query_in_header() {
        let dir = TestDir::new("lume_lsp_inter_query_preamble");
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::create_dir_all(dir.path().join("include")).unwrap();
        std::fs::write(dir.path().join("compile_flags.txt"), "-Iinclude\n").unwrap();

        let source_path = dir.path().join("src/t.c");
        let header_path = dir.path().join("include/arch.h");
        let source_text = "#include \"arch.h\"\nstruct Arch item;\n";
        let header_v0 = "struct Arch { int v; };\n";
        let header_v1 = "\n\nstruct Arch { int v; };\n";
        std::fs::write(&source_path, source_text).unwrap();
        std::fs::write(&header_path, header_v0).unwrap();

        let source_uri = path_to_uri(&source_path).unwrap();
        let project = ProjectKey {
            server: ServerKind::Clangd,
            root: dir.path().to_path_buf(),
        };

        let (mut session, tx, sent) = make_session(dir.path(), 2, "utf-16");
        let mut sync = DocumentSyncState::new();

        // 1. Query from t.c opens both arch.h and t.c
        tx.send(IncomingMessage::Message(
            json!({ "jsonrpc": "2.0", "id": 2, "result": null }),
        ))
        .unwrap();
        let q1 = DefinitionQuery {
            id: 1,
            source_path: source_path.clone(),
            source_byte: source_text.find("Arch").unwrap(),
            explorer_root: dir.path().to_path_buf(),
            documents: vec![
                OpenDocument {
                    path: source_path.clone(),
                    language: Language::C,
                    tab_id: 1,
                    revision: 0,
                    text: RopeBuffer::from_str(source_text),
                },
                OpenDocument {
                    path: header_path.clone(),
                    language: Language::C,
                    tab_id: 2,
                    revision: 0,
                    text: RopeBuffer::from_str(header_v0),
                },
            ],
        };
        sync.query_definition(&mut session, &project, &q1, "c", &source_path)
            .unwrap();
        let _ = take_sent_messages(&sent);

        // 2. User edits arch.h and queries FROM arch.h first (flushes arch.h didChange!)
        tx.send(IncomingMessage::Message(
            json!({ "jsonrpc": "2.0", "id": 3, "result": null }),
        ))
        .unwrap();
        let q2 = DefinitionQuery {
            id: 2,
            source_path: header_path.clone(),
            source_byte: header_v1.find("Arch").unwrap(),
            explorer_root: dir.path().to_path_buf(),
            documents: vec![
                OpenDocument {
                    path: source_path.clone(),
                    language: Language::C,
                    tab_id: 1,
                    revision: 0,
                    text: RopeBuffer::from_str(source_text),
                },
                OpenDocument {
                    path: header_path.clone(),
                    language: Language::C,
                    tab_id: 2,
                    revision: 1,
                    text: RopeBuffer::from_str(header_v1),
                },
            ],
        };
        sync.query_definition(&mut session, &project, &q2, "c", &header_path)
            .unwrap();
        let _ = take_sent_messages(&sent);

        // 3. User switches back to t.c and queries again: t.c must STILL cycle didClose+didOpen
        //    because arch.h changed after t.c's preamble was last opened!
        tx.send(IncomingMessage::Message(
            json!({ "jsonrpc": "2.0", "id": 4, "result": null }),
        ))
        .unwrap();
        let q3 = DefinitionQuery {
            id: 3,
            source_path: source_path.clone(),
            source_byte: source_text.find("Arch").unwrap(),
            explorer_root: dir.path().to_path_buf(),
            documents: vec![
                OpenDocument {
                    path: source_path.clone(),
                    language: Language::C,
                    tab_id: 1,
                    revision: 0,
                    text: RopeBuffer::from_str(source_text),
                },
                OpenDocument {
                    path: header_path.clone(),
                    language: Language::C,
                    tab_id: 2,
                    revision: 1,
                    text: RopeBuffer::from_str(header_v1),
                },
            ],
        };
        sync.query_definition(&mut session, &project, &q3, "c", &source_path)
            .unwrap();
        let msgs = take_sent_messages(&sent);
        assert_eq!(
            msgs.len(),
            3,
            "expected didClose, didOpen, definition; got {msgs:?}"
        );
        assert_eq!(msgs[0]["method"], "textDocument/didClose");
        assert_eq!(msgs[0]["params"]["textDocument"]["uri"], source_uri);
        assert_eq!(msgs[1]["method"], "textDocument/didOpen");
        assert_eq!(msgs[1]["params"]["textDocument"]["uri"], source_uri);
        assert_eq!(msgs[2]["method"], "textDocument/definition");
        assert_eq!(msgs[2]["params"]["textDocument"]["uri"], source_uri);
    }
}
