use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crossterm::terminal::size as terminal_size;

use crate::args::{parse_cli_args, CliCommand, CliTargets};
use crate::explorer::FileExplorer;
use crate::prompt::InputPrompt;
use crate::search::SearchState;
use crate::vim::VimState;
use lume_core::{Editor, TextBuffer};
use lume_syntax::{HighlightToken, Language, SyntaxRequest, SyntaxWorker};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveFocus {
    Editor,
    Explorer,
}

pub struct App {
    pub editor: Editor,
    pub explorer: FileExplorer,
    pub focus: ActiveFocus,
    pub scroll_row: usize,
    pub scroll_col: usize,
    pub terminal_width: u16,
    pub terminal_height: u16,
    pub status_message: Option<(String, Instant)>,
    pub running: bool,
    pub show_line_numbers: bool,

    // Mouse interaction
    pub mouse_dragging: bool,
    pub last_click_time: Instant,
    pub last_click_pos: (u16, u16),
    pub click_count: u8,

    // Overlays: Command Palette, Which-Key & Input Prompt
    pub palette_open: bool,
    pub palette_query: String,
    pub palette_selected: usize,
    pub which_key_open: bool,
    pub prompt: Option<InputPrompt>,
    pub search_state: Option<SearchState>,
    pub last_search_query: Option<String>,

    // Syntax worker & cache
    pub syntax_worker: SyntaxWorker,
    pub syntax_cache: HashMap<usize, (u64, Arc<Vec<HighlightToken>>)>,
    pub syntax_in_flight: HashMap<usize, u64>,

    // Modal Mode (Vim Motions)
    pub modal_enabled: bool,
    pub vim_state: VimState,

    // Clipboard
    pub pending_osc52: Option<String>,
}

impl App {
    #[allow(dead_code)]
    pub fn new(file_arg: Option<PathBuf>) -> Result<Self, std::io::Error> {
        let targets = match file_arg {
            Some(path) => {
                if path.is_dir() {
                    CliTargets {
                        workspace: Some(path),
                        files: Vec::new(),
                    }
                } else {
                    CliTargets {
                        workspace: None,
                        files: vec![path],
                    }
                }
            }
            None => CliTargets::default(),
        };
        Self::open_targets(targets)
    }

    #[allow(dead_code)]
    pub fn from_paths<P: AsRef<std::path::Path>>(paths: &[P]) -> Result<Self, std::io::Error> {
        let str_args: Vec<String> = std::iter::once("lume".to_string())
            .chain(
                paths
                    .iter()
                    .map(|p| p.as_ref().to_string_lossy().to_string()),
            )
            .collect();
        match parse_cli_args(&str_args) {
            CliCommand::Run(targets) | CliCommand::BenchStartup(targets) => {
                Self::open_targets(targets)
            }
            _ => Self::open_targets(CliTargets::default()),
        }
    }

    pub fn open_targets(targets: CliTargets) -> Result<Self, std::io::Error> {
        let initial_cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        let workspace_root = if let Some(ref ws) = targets.workspace {
            let resolved = if ws.is_absolute() {
                ws.clone()
            } else {
                initial_cwd.join(ws)
            };
            std::fs::canonicalize(&resolved).unwrap_or(resolved)
        } else {
            std::fs::canonicalize(&initial_cwd).unwrap_or(initial_cwd.clone())
        };

        let mut resolved_files = Vec::new();
        for file in targets.files {
            if file.is_absolute() {
                resolved_files.push(file);
            } else if initial_cwd.join(&file).exists() {
                resolved_files.push(initial_cwd.join(&file));
            } else if workspace_root.join(&file).exists() || targets.workspace.is_some() {
                resolved_files.push(workspace_root.join(&file));
            } else {
                resolved_files.push(initial_cwd.join(&file));
            }
        }

        let mut editor = Editor::new();
        let mut initial_status = None;

        if !resolved_files.is_empty() {
            let mut first = true;
            for file_path in &resolved_files {
                if file_path.is_file() {
                    if first {
                        match editor.open_in_current_tab(file_path) {
                            Ok(()) => {}
                            Err(e) => {
                                initial_status =
                                    Some(format!("Error opening {}: {}", file_path.display(), e));
                                editor.set_file_path(Some(file_path.clone()));
                            }
                        }
                        first = false;
                    } else {
                        match editor.open_tab(file_path) {
                            Ok(_) => {}
                            Err(e) => {
                                initial_status =
                                    Some(format!("Error opening {}: {}", file_path.display(), e));
                                let _ = editor.new_tab();
                                editor.set_file_path(Some(file_path.clone()));
                            }
                        }
                    }
                } else if !file_path.exists() {
                    if first {
                        editor.set_file_path(Some(file_path.clone()));
                        first = false;
                    } else {
                        let _ = editor.new_tab();
                        editor.set_file_path(Some(file_path.clone()));
                    }
                }
            }
            editor.switch_tab(0);
        }

        let explorer = FileExplorer::new(workspace_root.clone());
        let (width, height) = terminal_size().unwrap_or((80, 24));

        let mut app = Self {
            editor,
            explorer,
            focus: ActiveFocus::Editor,
            scroll_row: 0,
            scroll_col: 0,
            terminal_width: width,
            terminal_height: height,
            status_message: None,
            running: true,
            show_line_numbers: true,

            mouse_dragging: false,
            last_click_time: Instant::now(),
            last_click_pos: (0, 0),
            click_count: 0,

            palette_open: false,
            palette_query: String::new(),
            palette_selected: 0,
            which_key_open: false,
            prompt: None,
            search_state: None,
            last_search_query: None,

            syntax_worker: SyntaxWorker::spawn(),
            syntax_cache: HashMap::new(),
            syntax_in_flight: HashMap::new(),

            modal_enabled: false,
            vim_state: VimState::new(),

            pending_osc52: None,
        };

        if let Some(msg) = initial_status {
            app.set_status(msg);
        } else if resolved_files.len() == 1 {
            if resolved_files[0].exists() {
                app.set_status(format!("Opened {}", resolved_files[0].display()));
            } else {
                app.set_status(format!("New file: {}", resolved_files[0].display()));
            }
        } else if resolved_files.len() > 1 {
            app.set_status(format!("Opened {} files", resolved_files.len()));
        } else if targets.workspace.is_some() {
            app.set_status(format!("Workspace: {}", workspace_root.display()));
        }

        Ok(app)
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some((msg.into(), Instant::now()));
    }

    pub fn update_search_query(&mut self, state: &mut SearchState) {
        self.last_search_query = if !state.query.is_empty() {
            Some(state.query.clone())
        } else {
            None
        };
        if state.query.is_empty() {
            state.match_count = 0;
            state.active_match_idx = 0;
            self.editor
                .cursors_mut()
                .set_cursors(vec![state.initial_cursor]);
            self.scroll_row = state.initial_scroll.0;
            self.scroll_col = state.initial_scroll.1;
        } else {
            state.match_count =
                lume_core::search::count_matches(self.editor.buffer(), &state.query, false);
            let from = state.initial_cursor.min();
            if let Some((s, e)) =
                lume_core::search::find_next(self.editor.buffer(), &state.query, from, false)
            {
                let primary = self.editor.cursors_mut().primary_mut();
                primary.anchor = s;
                primary.head = e;
                state.active_match_idx = lume_core::search::find_matches_in_range(
                    self.editor.buffer(),
                    &state.query,
                    0,
                    s,
                    false,
                )
                .len();
                self.adjust_viewport();
            } else {
                state.active_match_idx = 0;
            }
        }
    }

    pub fn explorer_width(&self) -> usize {
        if self.explorer.is_visible {
            self.explorer.width
        } else {
            0
        }
    }

    pub fn text_area_left(&self) -> usize {
        self.explorer_width()
    }

    pub fn text_area_top(&self) -> usize {
        1 // Row 0 is the Tab bar
    }

    pub fn text_area_height(&self) -> usize {
        // Row 0: Tab bar, Last row: Status bar
        self.terminal_height.saturating_sub(2) as usize
    }

    pub fn gutter_width(&self) -> usize {
        if !self.show_line_numbers {
            return 0;
        }
        let total_lines = self.editor.buffer().len_lines().max(1);
        let digits = format!("{}", total_lines).len();
        digits + 3 // digits + 1 space + '│' + 1 space
    }

    pub fn text_area_width(&self) -> usize {
        let gutter = self.gutter_width();
        let exp_w = self.explorer_width();
        (self.terminal_width as usize).saturating_sub(gutter + exp_w)
    }

    pub fn adjust_viewport(&mut self) {
        let (cursor_row, cursor_col) = self.editor.cursor_line_col();
        let height = self.text_area_height().max(1);
        let width = self.text_area_width().max(1);

        if cursor_row < self.scroll_row {
            self.scroll_row = cursor_row;
        } else if cursor_row >= self.scroll_row + height {
            self.scroll_row = cursor_row.saturating_sub(height - 1);
        }

        if cursor_col < self.scroll_col {
            self.scroll_col = cursor_col;
        } else if cursor_col >= self.scroll_col + width {
            self.scroll_col = cursor_col.saturating_sub(width - 1);
        }
    }

    pub fn current_language(&self) -> Option<Language> {
        if let Some(override_id) = self.editor.language_override() {
            Language::from_id(override_id)
        } else {
            self.editor.file_path().and_then(Language::from_path)
        }
    }

    pub fn has_pending_syntax_response(&mut self) -> bool {
        let mut updated = false;
        while let Some(res) = self.syntax_worker.try_recv_response() {
            let is_newer = self
                .syntax_cache
                .get(&res.buffer_id)
                .map(|(rev, _)| res.revision >= *rev)
                .unwrap_or(true);
            if is_newer {
                self.syntax_cache
                    .insert(res.buffer_id, (res.revision, Arc::new(res.tokens)));
                self.syntax_in_flight.remove(&res.buffer_id);
                if res.buffer_id == self.editor.active_tab().id {
                    updated = true;
                }
            }
        }
        updated
    }

    pub fn poll_async_saves(&mut self) -> bool {
        let events = self.editor.poll_async_saves();
        let mut updated = false;
        for ev in events {
            updated = true;
            match ev.result {
                Ok(()) => {
                    self.set_status("Autosaved");
                }
                Err(err) => {
                    self.set_status(format!("Save error: {}", err));
                }
            }
        }
        updated
    }

    pub fn update_syntax_highlights(&mut self) -> Arc<Vec<HighlightToken>> {
        let _ = self.has_pending_syntax_response();

        let active_tab = self.editor.active_tab();
        let tab_id = active_tab.id;
        let tab_rev = active_tab.revision;

        let needs_request = match self.syntax_cache.get(&tab_id) {
            Some((cached_rev, _)) => *cached_rev != tab_rev,
            None => true,
        };

        if needs_request && self.syntax_in_flight.get(&tab_id) != Some(&tab_rev) {
            if let Some(lang) = self.current_language() {
                let full_text = self.editor.buffer().rope().to_string();
                let full_len = full_text.len();
                self.syntax_in_flight.insert(tab_id, tab_rev);
                self.syntax_worker.send_request(SyntaxRequest {
                    buffer_id: tab_id,
                    revision: tab_rev,
                    language: lang,
                    text: full_text,
                    visible_range: 0..full_len,
                });
            }
        }

        self.syntax_cache
            .get(&tab_id)
            .map(|(_, tokens)| Arc::clone(tokens))
            .unwrap_or_else(|| Arc::new(Vec::new()))
    }

    pub fn handle_deleted_folder_tabs(&mut self, folder_path: &std::path::Path) {
        let folder_canon = folder_path.canonicalize().unwrap_or_else(|_| {
            if folder_path.is_absolute() {
                folder_path.to_path_buf()
            } else {
                self.explorer.root.join(folder_path)
            }
        });

        for tab in self.editor.tabs_mut() {
            if let Some(ref p) = tab.file_path {
                let p_abs = if p.is_absolute() {
                    p.clone()
                } else {
                    self.explorer.root.join(p)
                };
                let p_canon = p.canonicalize().unwrap_or_else(|_| p_abs.clone());

                let is_inside = p_canon.starts_with(&folder_canon)
                    || p_abs.starts_with(&folder_canon)
                    || p_abs.starts_with(folder_path);

                if is_inside {
                    let _ = lume_core::Journal::remove(p);
                    let _ = lume_core::Journal::remove(&p_canon);
                    let _ = lume_core::Journal::remove(&p_abs);
                    tab.file_path = None;
                    tab.is_dirty = true;
                }
            }
        }
    }

    pub fn handle_renamed_path_tabs(
        &mut self,
        old_path: &std::path::Path,
        new_path: &std::path::Path,
    ) {
        let old_canon = old_path.canonicalize().unwrap_or_else(|_| {
            if old_path.is_absolute() {
                old_path.to_path_buf()
            } else {
                self.explorer.root.join(old_path)
            }
        });
        let new_canon = new_path.canonicalize().unwrap_or_else(|_| {
            if new_path.is_absolute() {
                new_path.to_path_buf()
            } else {
                self.explorer.root.join(new_path)
            }
        });
        let old_parent_canon = old_path.parent().and_then(|p| p.canonicalize().ok());

        for tab in self.editor.tabs_mut() {
            if let Some(ref p) = tab.file_path {
                let p_abs = if p.is_absolute() {
                    p.clone()
                } else {
                    self.explorer.root.join(p)
                };
                let p_canon = p.canonicalize().unwrap_or_else(|_| p_abs.clone());
                let p_parent_canon = p_abs.parent().and_then(|par| par.canonicalize().ok());

                let is_exact_match = p_canon == old_canon
                    || p_abs == old_canon
                    || p_abs == old_path
                    || (p.file_name() == old_path.file_name()
                        && old_parent_canon.is_some()
                        && old_parent_canon == p_parent_canon);

                if is_exact_match {
                    let _ = lume_core::Journal::remove(p);
                    let _ = lume_core::Journal::remove(&p_canon);
                    let _ = lume_core::Journal::remove(&p_abs);
                    tab.file_path = Some(new_canon.clone());
                    self.syntax_cache.remove(&tab.id);
                    self.syntax_in_flight.remove(&tab.id);
                } else if p_canon.starts_with(&old_canon)
                    || p_abs.starts_with(&old_canon)
                    || p_abs.starts_with(old_path)
                {
                    let rel = p_canon
                        .strip_prefix(&old_canon)
                        .or_else(|_| p_abs.strip_prefix(&old_canon))
                        .or_else(|_| p_abs.strip_prefix(old_path));
                    if let Ok(rel) = rel {
                        let updated = new_canon.join(rel);
                        let _ = lume_core::Journal::remove(p);
                        let _ = lume_core::Journal::remove(&p_canon);
                        let _ = lume_core::Journal::remove(&p_abs);
                        tab.file_path = Some(updated);
                        self.syntax_cache.remove(&tab.id);
                        self.syntax_in_flight.remove(&tab.id);
                    }
                }
            }
        }
    }
}
