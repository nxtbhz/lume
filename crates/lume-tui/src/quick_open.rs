use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use lume_core::fuzzy::fuzzy_match;

/// A workspace file entry indexed for Quick Open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickOpenItem {
    /// Relative path from workspace root, with `/` forward-slash separators.
    pub relative_path: String,
    /// Simple filename without directory components.
    pub file_name: String,
    /// Absolute full path to the file on disk.
    pub full_path: PathBuf,
    /// Whether this file is currently open in an editor tab.
    pub is_open_tab: bool,
}

/// A matched item with its score and highlighted character positions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickOpenMatch {
    /// Index in `QuickOpenState::files`.
    pub item_idx: usize,
    /// Matching score (higher is better).
    pub score: i64,
    /// Character indices in `relative_path` that matched the query.
    pub match_indices: Vec<usize>,
}

/// State machine for the Quick Open modal overlay.
#[derive(Debug, Clone)]
pub struct QuickOpenState {
    /// Active query string.
    pub query: String,
    /// Cursor position in characters within `query`.
    pub cursor_pos: usize,
    /// All indexed workspace files.
    pub files: Vec<QuickOpenItem>,
    /// Currently filtered and scored matches.
    pub filtered: Vec<QuickOpenMatch>,
    /// Selected index in `filtered`.
    pub selected_idx: usize,
    /// Vertical scroll offset in `filtered`.
    pub scroll_offset: usize,
}

impl QuickOpenState {
    /// Creates a new `QuickOpenState` and performs initial ranking.
    pub fn new(files: Vec<QuickOpenItem>) -> Self {
        let mut state = Self {
            query: String::new(),
            cursor_pos: 0,
            files,
            filtered: Vec::new(),
            selected_idx: 0,
            scroll_offset: 0,
        };
        state.update_filter();
        state
    }

    /// Re-evaluates matches against current query.
    pub fn update_filter(&mut self) {
        let trimmed = self.query.trim();

        if trimmed.is_empty() {
            let mut matches: Vec<QuickOpenMatch> = self
                .files
                .iter()
                .enumerate()
                .map(|(idx, item)| {
                    let score = if item.is_open_tab { 1000 } else { 0 };
                    QuickOpenMatch {
                        item_idx: idx,
                        score,
                        match_indices: Vec::new(),
                    }
                })
                .collect();

            // When query is empty, sort open tabs first, then alphabetically by relative_path
            matches.sort_by(|a, b| {
                b.score.cmp(&a.score).then_with(|| {
                    self.files[a.item_idx]
                        .relative_path
                        .cmp(&self.files[b.item_idx].relative_path)
                })
            });

            self.filtered = matches;
        } else {
            let mut matches = Vec::new();
            for (idx, item) in self.files.iter().enumerate() {
                if let Some(m) = fuzzy_match(&self.query, &item.relative_path) {
                    let tab_bonus = if item.is_open_tab { 50 } else { 0 };
                    matches.push(QuickOpenMatch {
                        item_idx: idx,
                        score: m.score + tab_bonus,
                        match_indices: m.match_indices,
                    });
                }
            }

            // Sort matches descending by score, then alphabetically
            matches.sort_by(|a, b| {
                b.score.cmp(&a.score).then_with(|| {
                    self.files[a.item_idx]
                        .relative_path
                        .cmp(&self.files[b.item_idx].relative_path)
                })
            });

            self.filtered = matches;
        }

        if self.filtered.is_empty() {
            self.selected_idx = 0;
            self.scroll_offset = 0;
        } else if self.selected_idx >= self.filtered.len() {
            self.selected_idx = self.filtered.len() - 1;
        }
    }

    /// Handles typing a character into the query.
    pub fn handle_char(&mut self, c: char) {
        let mut chars: Vec<char> = self.query.chars().collect();
        if self.cursor_pos > chars.len() {
            self.cursor_pos = chars.len();
        }
        chars.insert(self.cursor_pos, c);
        self.cursor_pos += 1;
        self.query = chars.into_iter().collect();
        self.update_filter();
    }

    /// Handles deleting a character before the cursor.
    pub fn handle_backspace(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        let mut chars: Vec<char> = self.query.chars().collect();
        if self.cursor_pos <= chars.len() {
            chars.remove(self.cursor_pos - 1);
            self.cursor_pos -= 1;
            self.query = chars.into_iter().collect();
            self.update_filter();
        }
    }

    /// Handles pasting text into the query, sanitizing to a single line.
    pub fn handle_paste(&mut self, text: &str) {
        let single_line = text
            .lines()
            .next()
            .unwrap_or("")
            .trim_matches(&['\r', '\n'][..]);
        if single_line.is_empty() {
            return;
        }
        let mut chars: Vec<char> = self.query.chars().collect();
        let paste_chars: Vec<char> = single_line.chars().collect();
        let paste_len = paste_chars.len();
        if self.cursor_pos > chars.len() {
            self.cursor_pos = chars.len();
        }
        chars.splice(self.cursor_pos..self.cursor_pos, paste_chars);
        self.cursor_pos += paste_len;
        self.query = chars.into_iter().collect();
        self.update_filter();
    }

    /// Moves selection up.
    pub fn move_up(&mut self) {
        if self.selected_idx > 0 {
            self.selected_idx -= 1;
            if self.selected_idx < self.scroll_offset {
                self.scroll_offset = self.selected_idx;
            }
        }
    }

    /// Moves selection down, scrolling if necessary.
    pub fn move_down(&mut self, max_visible: usize) {
        if !self.filtered.is_empty() && self.selected_idx + 1 < self.filtered.len() {
            self.selected_idx += 1;
            if max_visible > 0 && self.selected_idx >= self.scroll_offset + max_visible {
                self.scroll_offset = self.selected_idx.saturating_sub(max_visible - 1);
            }
        }
    }

    /// Returns a reference to the currently selected item, if any.
    pub fn selected_item(&self) -> Option<&QuickOpenItem> {
        self.filtered
            .get(self.selected_idx)
            .map(|m| &self.files[m.item_idx])
    }
}

/// Binary and artifact file extensions to skip during workspace scan.
const IGNORED_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "ico", "webp", "pdf", "zip", "tar", "gz", "7z", "mp3", "mp4",
    "exe", "bin", "dll", "so", "dylib", "o", "a", "rlib", "wasm", "class", "pyc", "profraw",
    "profdata",
];

/// Directory names to ignore during workspace scan.
const IGNORED_DIRS: &[&str] = &[
    "target",
    "node_modules",
    "dist",
    "build",
    "out",
    "vendor",
    ".lume_tmp",
    "coverage",
];

/// Scans workspace files under `root`, excluding artifacts and binaries.
///
/// Bounded to `max_files` entries and max recursion depth 32 with cycle detection.
pub fn scan_workspace_files(
    root: &Path,
    open_tab_paths: &[PathBuf],
    max_files: usize,
) -> Vec<QuickOpenItem> {
    let canon_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut visited_dirs = HashSet::new();
    let mut items = Vec::new();

    let open_tabs_canon: HashSet<PathBuf> = open_tab_paths
        .iter()
        .map(|p| p.canonicalize().unwrap_or_else(|_| p.clone()))
        .collect();

    let open_tabs_raw: HashSet<PathBuf> = open_tab_paths.iter().cloned().collect();

    let ctx = ScanContext {
        canon_root: &canon_root,
        orig_root: root,
        open_tabs_raw: &open_tabs_raw,
        open_tabs_canon: &open_tabs_canon,
        max_files,
    };

    scan_dir_recursive(root, &ctx, &mut visited_dirs, &mut items, 0);

    // Ensure all currently open tabs are included in items (even if hidden or outside workspace root)
    for tab_path in open_tab_paths {
        if !items.iter().any(|it| it.full_path == *tab_path) && tab_path.exists() {
            let file_name = tab_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "untitled".to_string());
            let rel_path = tab_path
                .strip_prefix(root)
                .or_else(|_| tab_path.strip_prefix(&canon_root))
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| tab_path.to_string_lossy().replace('\\', "/"));

            items.insert(
                0,
                QuickOpenItem {
                    relative_path: rel_path,
                    file_name,
                    full_path: tab_path.clone(),
                    is_open_tab: true,
                },
            );
        }
    }

    items
}

struct ScanContext<'a> {
    canon_root: &'a Path,
    orig_root: &'a Path,
    open_tabs_raw: &'a HashSet<PathBuf>,
    open_tabs_canon: &'a HashSet<PathBuf>,
    max_files: usize,
}

fn scan_dir_recursive(
    dir: &Path,
    ctx: &ScanContext<'_>,
    visited_dirs: &mut HashSet<PathBuf>,
    items: &mut Vec<QuickOpenItem>,
    depth: usize,
) {
    if depth > 32 || items.len() >= ctx.max_files {
        return;
    }

    let canon_dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    if !visited_dirs.insert(canon_dir) {
        // Cycle detected
        return;
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut subdirs = Vec::new();

    for entry in entries.flatten() {
        if items.len() >= ctx.max_files {
            break;
        }

        let path = entry.path();
        let file_name_os = entry.file_name();
        let file_name_str = file_name_os.to_string_lossy();

        // Skip hidden files/directories starting with '.'
        if file_name_str.starts_with('.') {
            continue;
        }

        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        if file_type.is_dir() {
            if IGNORED_DIRS.contains(&file_name_str.as_ref()) {
                continue;
            }
            subdirs.push(path);
        } else if file_type.is_file() {
            // Check extension
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                let ext_lower = ext.to_ascii_lowercase();
                if IGNORED_EXTENSIONS.contains(&ext_lower.as_str()) {
                    continue;
                }
            }

            // Compute relative path
            let rel_path = path
                .strip_prefix(ctx.orig_root)
                .or_else(|_| path.strip_prefix(ctx.canon_root))
                .unwrap_or(&path);

            let rel_str = rel_path.to_string_lossy().replace('\\', "/");

            let is_open_tab = ctx.open_tabs_raw.contains(&path)
                || (!ctx.open_tabs_canon.is_empty()
                    && path
                        .canonicalize()
                        .is_ok_and(|c| ctx.open_tabs_canon.contains(&c)));

            items.push(QuickOpenItem {
                relative_path: rel_str,
                file_name: file_name_str.into_owned(),
                full_path: path,
                is_open_tab,
            });
        }
    }

    // Sort subdirectories for deterministic scan order
    subdirs.sort();

    for subdir in subdirs {
        scan_dir_recursive(&subdir, ctx, visited_dirs, items, depth + 1);
    }
}
