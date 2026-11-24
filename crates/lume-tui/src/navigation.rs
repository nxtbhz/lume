use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::app::{ActiveFocus, App};
use crate::lsp::project::{canonicalize_best_effort, server_for};
use crate::lsp::protocol::byte_at_position;
use crate::lsp::{DefinitionQuery, DefinitionReply, DefinitionTarget, LspError, OpenDocument};
use crate::prompt::InputPrompt;

/// Snapshot of editor state when a Go to Definition request was initiated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefinitionOrigin {
    pub request_id: u64,
    pub tab_id: usize,
    pub revision: u64,
    pub path: PathBuf,
    pub cursor_byte: usize,
    pub pane: usize,
}

/// In-flight Go to Definition state used to validate responses when they arrive.
#[derive(Clone, Debug)]
pub struct PendingDefinition {
    pub origin: DefinitionOrigin,
    pub doc_revisions: BTreeMap<PathBuf, (usize, u64)>,
}

impl App {
    /// Initiates an asynchronous Go to Definition lookup at the primary cursor.
    pub fn request_definition_at_cursor(&mut self) {
        if self.focus != ActiveFocus::Editor {
            self.set_status("Focus the editor to Go to Definition");
            return;
        }

        let active_tab_idx = self.editor.active_tab_idx();
        let Some(source_path) = self.editor.file_path().map(Path::to_path_buf) else {
            self.set_status("Go to Definition requires a saved file path");
            return;
        };

        let Some(source_lang) = self.language_for_tab(active_tab_idx) else {
            self.set_status("Go to Definition is only supported for C/C++ files");
            return;
        };
        if server_for(&source_path, source_lang).is_none() {
            self.set_status("Go to Definition is only supported for C/C++ files");
            return;
        }

        let explorer_root = self.explorer.root.clone();
        let active_tab = self.editor.active_tab();
        let cursor_byte = self.editor.cursors().primary().head;
        let pane = self.splits.active_pane;

        self.next_definition_request_id = self.next_definition_request_id.wrapping_add(1);
        let request_id = self.next_definition_request_id;

        let origin = DefinitionOrigin {
            request_id,
            tab_id: active_tab.id,
            revision: active_tab.revision,
            path: source_path.clone(),
            cursor_byte,
            pane,
        };

        let mut documents = Vec::new();
        let mut doc_revisions = BTreeMap::new();
        for (idx, tab) in self.editor.tabs().iter().enumerate() {
            let Some(ref path) = tab.file_path else {
                continue;
            };
            let Some(language) = self.language_for_tab(idx) else {
                continue;
            };
            let canon = canonicalize_best_effort(path, &explorer_root);
            doc_revisions.insert(canon, (tab.id, tab.revision));
            documents.push(OpenDocument {
                path: path.clone(),
                language,
                tab_id: tab.id,
                revision: tab.revision,
                text: tab.buffer.clone(),
            });
        }

        let query = DefinitionQuery {
            id: request_id,
            source_path,
            source_byte: cursor_byte,
            explorer_root,
            documents,
        };

        self.pending_definition = Some(PendingDefinition {
            origin,
            doc_revisions,
        });

        match self.lsp_client.request_definition(query) {
            Ok(()) => self.set_status("Looking up definition…"),
            Err(err) => {
                self.pending_definition = None;
                self.set_status(err.to_string());
            }
        }
    }

    /// Processes a completed definition reply delivered through [`crate::input::AppEvent::Definition`].
    pub fn handle_definition_reply(&mut self, reply: DefinitionReply) {
        let Some(pending) = self.pending_definition.as_ref() else {
            return;
        };
        if pending.origin.request_id != reply.id {
            // Older out-of-order reply; keep waiting for the newer active request.
            return;
        }
        let pending = self.pending_definition.take().expect("checked above");

        if self.has_blocking_overlay() || !self.origin_matches(&pending.origin) {
            return;
        }

        match reply.result {
            Err(LspError::ContentModified) => {
                self.set_status("Document changed during definition lookup; please retry");
            }
            Err(err) => {
                self.set_status(err.to_string());
            }
            Ok(targets) if targets.is_empty() => {
                self.set_status("No definition found");
            }
            Ok(targets) if targets.len() == 1 => {
                let target = &targets[0];
                if self.target_changed_since_query(target, &pending.doc_revisions) {
                    self.set_status("Target file changed since lookup; repeat Go to Definition");
                    return;
                }
                if let Err(err) = self.jump_to_definition(target) {
                    self.set_status(format!("Go to Definition failed: {err}"));
                }
            }
            Ok(targets) => {
                let labels: Vec<String> = targets
                    .iter()
                    .map(|t| self.format_definition_label(t))
                    .collect();
                self.prompt = Some(InputPrompt::select_definition(
                    pending.origin,
                    pending.doc_revisions,
                    targets,
                    labels,
                ));
            }
        }
    }

    pub(crate) fn accept_definition_selection(
        &mut self,
        origin: DefinitionOrigin,
        doc_revisions: BTreeMap<PathBuf, (usize, u64)>,
        targets: Vec<DefinitionTarget>,
        selected_idx: usize,
    ) {
        if !self.origin_matches(&origin) {
            self.set_status("Origin changed; repeat Go to Definition");
            return;
        }
        let Some(target) = targets.get(selected_idx) else {
            self.set_status("No definition found");
            return;
        };
        if self.target_changed_since_query(target, &doc_revisions) {
            self.set_status("Target file changed since lookup; repeat Go to Definition");
            return;
        }
        if let Err(err) = self.jump_to_definition(target) {
            self.set_status(format!("Go to Definition failed: {err}"));
        }
    }

    /// Opens or reuses the tab for `target` and places a single cursor at the resolved byte offset.
    pub fn jump_to_definition(&mut self, target: &DefinitionTarget) -> io::Result<()> {
        let meta = std::fs::metadata(&target.path).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!("cannot open '{}': {e}", target.path.display()),
            )
        })?;
        if !meta.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("'{}' is not a regular file", target.path.display()),
            ));
        }

        let prev_active = self.editor.active_tab_idx();
        let tab_count_before = self.editor.tabs().len();
        let target_idx = self.open_file_into_editor_result(&target.path)?;

        let Some(target_byte) =
            byte_at_position(self.editor.buffer(), target.position, target.encoding)
        else {
            if self.editor.tabs().len() > tab_count_before {
                self.close_tab_and_sync_splits(target_idx);
            }
            self.editor.switch_tab(prev_active);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "definition position {}:{} out of range for '{}'",
                    target.position.line + 1,
                    target.position.character + 1,
                    target.path.display()
                ),
            ));
        };

        self.editor.cursors_mut().set_single(target_byte);
        self.focus = ActiveFocus::Editor;
        self.adjust_viewport();
        self.refresh_git_head();
        self.set_status(format!(
            "Jumped to {}:{}",
            target.path.display(),
            target.position.line + 1
        ));

        if let Some(tab) = self.editor.tabs().get(target_idx) {
            if tab.pending_recovery.is_some() {
                self.definition_recovery_tab_id = Some(tab.id);
                self.prompt_recover_if_needed(target_idx);
            }
        }

        Ok(())
    }

    fn has_blocking_overlay(&self) -> bool {
        self.prompt.is_some()
            || self.search_state.is_some()
            || self.palette_open
            || self.which_key_open
            || self.quick_open.is_some()
    }

    fn origin_matches(&self, origin: &DefinitionOrigin) -> bool {
        if self.focus != ActiveFocus::Editor || self.splits.active_pane != origin.pane {
            return false;
        }
        let tab = self.editor.active_tab();
        if tab.id != origin.tab_id || tab.revision != origin.revision {
            return false;
        }
        if self.editor.cursors().primary().head != origin.cursor_byte {
            return false;
        }
        let Some(ref current_path) = tab.file_path else {
            return false;
        };
        let current_canon = canonicalize_best_effort(current_path, &self.explorer.root);
        let origin_canon = canonicalize_best_effort(&origin.path, &self.explorer.root);
        current_canon == origin_canon
    }

    fn target_changed_since_query(
        &self,
        target: &DefinitionTarget,
        doc_revisions: &BTreeMap<PathBuf, (usize, u64)>,
    ) -> bool {
        let target_canon = canonicalize_best_effort(&target.path, &self.explorer.root);
        for tab in self.editor.tabs() {
            let Some(ref path) = tab.file_path else {
                continue;
            };
            if canonicalize_best_effort(path, &self.explorer.root) == target_canon {
                return match doc_revisions.get(&target_canon) {
                    Some(&(old_tab_id, old_rev)) => tab.id != old_tab_id || tab.revision != old_rev,
                    None => tab.is_dirty || tab.revision > 0,
                };
            }
        }
        false
    }

    fn format_definition_label(&self, target: &DefinitionTarget) -> String {
        let display_path = target
            .path
            .strip_prefix(&self.explorer.root)
            .unwrap_or(&target.path);
        format!("{}:{}", display_path.display(), target.position.line + 1)
    }
}
