use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct ExplorerItem {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub depth: usize,
    pub is_expanded: bool,
}

pub struct FileExplorer {
    pub root: PathBuf,
    pub expanded_dirs: HashSet<PathBuf>,
    pub items: Vec<ExplorerItem>,
    pub selected_idx: usize,
    pub scroll_offset: usize,
    pub is_visible: bool,
    pub width: usize,
}

impl FileExplorer {
    pub fn new(root: PathBuf) -> Self {
        let root = root.canonicalize().unwrap_or(root);
        let mut expanded_dirs = HashSet::new();
        expanded_dirs.insert(root.clone());

        let mut explorer = Self {
            root,
            expanded_dirs,
            items: Vec::new(),
            selected_idx: 0,
            scroll_offset: 0,
            is_visible: true,
            width: 26,
        };
        explorer.refresh();
        explorer
    }

    pub fn refresh(&mut self) {
        let mut items = Vec::new();
        Self::populate_tree(&self.root, 0, &self.expanded_dirs, &mut items);
        self.items = items;
        if self.items.is_empty() {
            self.selected_idx = 0;
            self.scroll_offset = 0;
        } else {
            if self.selected_idx >= self.items.len() {
                self.selected_idx = self.items.len() - 1;
            }
            if self.scroll_offset >= self.items.len() {
                self.scroll_offset = self.selected_idx;
            }
            if self.selected_idx < self.scroll_offset {
                self.scroll_offset = self.selected_idx;
            }
        }
    }

    fn populate_tree(
        dir: &Path,
        depth: usize,
        expanded_dirs: &HashSet<PathBuf>,
        out: &mut Vec<ExplorerItem>,
    ) {
        let read_dir = match fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(_) => return,
        };

        let mut entries: Vec<fs::DirEntry> = read_dir.filter_map(|e| e.ok()).collect();

        // Sort: directories first, then alphabetically
        entries.sort_by(|a, b| {
            let a_is_dir = a.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            let b_is_dir = b.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            match (a_is_dir, b_is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.file_name().cmp(&b.file_name()),
            }
        });

        for entry in entries {
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();

            // Ignore hidden files and build/repo artifacts
            if file_name.starts_with('.') && file_name != ".gitignore" {
                continue;
            }
            if file_name == "target"
                || file_name == "node_modules"
                || file_name.ends_with(".lume_tmp")
            {
                continue;
            }

            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            let is_expanded = is_dir && expanded_dirs.contains(&path);

            out.push(ExplorerItem {
                path: path.clone(),
                name: file_name,
                is_dir,
                depth,
                is_expanded,
            });

            if is_dir && is_expanded {
                Self::populate_tree(&path, depth + 1, expanded_dirs, out);
            }
        }
    }

    pub fn toggle_visible(&mut self) {
        self.is_visible = !self.is_visible;
        if self.is_visible {
            self.refresh();
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_idx > 0 {
            self.selected_idx -= 1;
            if self.selected_idx < self.scroll_offset {
                self.scroll_offset = self.selected_idx;
            }
        }
    }

    pub fn move_down(&mut self, max_rows: usize) {
        if !self.items.is_empty() && self.selected_idx + 1 < self.items.len() {
            self.selected_idx += 1;
            if self.selected_idx >= self.scroll_offset + max_rows {
                self.scroll_offset = self.selected_idx.saturating_sub(max_rows - 1);
            }
        }
    }

    pub fn activate_selected(&mut self) -> Option<PathBuf> {
        if self.items.is_empty() || self.selected_idx >= self.items.len() {
            return None;
        }

        let item = &self.items[self.selected_idx];
        if item.is_dir {
            let path = item.path.clone();
            if self.expanded_dirs.contains(&path) {
                self.expanded_dirs.remove(&path);
            } else {
                self.expanded_dirs.insert(path);
            }
            self.refresh();
            None
        } else {
            Some(item.path.clone())
        }
    }

    pub fn collapse_selected(&mut self) {
        if self.items.is_empty() || self.selected_idx >= self.items.len() {
            return;
        }
        let item = &self.items[self.selected_idx];
        if item.is_dir && item.is_expanded {
            let path = item.path.clone();
            self.expanded_dirs.remove(&path);
            self.refresh();
        } else if item.depth > 0 {
            // Find parent directory
            let target_depth = item.depth - 1;
            for i in (0..self.selected_idx).rev() {
                if self.items[i].is_dir && self.items[i].depth == target_depth {
                    self.selected_idx = i;
                    if self.selected_idx < self.scroll_offset {
                        self.scroll_offset = self.selected_idx;
                    }
                    break;
                }
            }
        }
    }

    pub fn expand_selected(&mut self) {
        if self.items.is_empty() || self.selected_idx >= self.items.len() {
            return;
        }
        let item = &self.items[self.selected_idx];
        if item.is_dir && !item.is_expanded {
            let path = item.path.clone();
            self.expanded_dirs.insert(path);
            self.refresh();
        }
    }

    pub fn click_at(&mut self, row_relative: usize) -> Option<PathBuf> {
        let target_idx = self.scroll_offset + row_relative;
        if target_idx < self.items.len() {
            self.selected_idx = target_idx;
            self.activate_selected()
        } else {
            None
        }
    }

    pub fn selected_item(&self) -> Option<&ExplorerItem> {
        self.items.get(self.selected_idx)
    }

    pub fn target_folder_for_creation(&self) -> PathBuf {
        if let Some(item) = self.selected_item() {
            if item.is_dir {
                item.path.clone()
            } else {
                item.path.parent().unwrap_or(&self.root).to_path_buf()
            }
        } else {
            self.root.clone()
        }
    }

    pub fn create_folder<P: AsRef<Path>>(
        &mut self,
        base_dir: &Path,
        rel_path: P,
    ) -> std::io::Result<PathBuf> {
        let rel = rel_path.as_ref();
        let rel_str = rel.to_string_lossy();
        let trimmed_input = rel_str.trim();
        if trimmed_input.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Folder name cannot be empty",
            ));
        }

        // Check if raw input is absolute or contains drive prefixes
        if rel.is_absolute()
            || trimmed_input.starts_with('/')
            || trimmed_input.starts_with('\\')
            || Path::new(trimmed_input).components().any(|c| {
                matches!(
                    c,
                    std::path::Component::Prefix(_) | std::path::Component::RootDir
                )
            })
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Absolute paths or drive prefixes are forbidden",
            ));
        }

        let trimmed = trimmed_input.trim_end_matches(['/', '\\']);
        if trimmed.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Folder name cannot be empty",
            ));
        }

        // Prevent escaping root workspace
        let root_canon = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone());
        if let Ok(canon_base) = base_dir.canonicalize() {
            if !canon_base.starts_with(&root_canon) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Base directory is outside workspace root",
                ));
            }
        }

        let mut depth_check = base_dir.to_path_buf();
        for comp in Path::new(trimmed).components() {
            match comp {
                std::path::Component::Prefix(_) | std::path::Component::RootDir => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "Absolute paths or drive prefixes are forbidden",
                    ));
                }
                std::path::Component::ParentDir => {
                    if !depth_check.pop() || !depth_check.starts_with(&root_canon) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            "Path traversal escaping workspace root is forbidden",
                        ));
                    }
                }
                std::path::Component::Normal(c) => depth_check.push(c),
                _ => {}
            }
        }

        let normalized = if base_dir.is_absolute() || base_dir.starts_with(&self.root) {
            base_dir.join(trimmed)
        } else {
            self.root.join(base_dir).join(trimmed)
        };

        fs::create_dir_all(&normalized)?;
        let canon_target = normalized.canonicalize().unwrap_or(normalized.clone());

        if !canon_target.starts_with(&root_canon) {
            let _ = fs::remove_dir_all(&canon_target);
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Target directory resolved outside workspace root",
            ));
        }

        // Auto-expand all ancestor directories from root down to created folder
        let mut curr = canon_target.parent();
        while let Some(parent) = curr {
            self.expanded_dirs.insert(parent.to_path_buf());
            if parent == root_canon || !parent.starts_with(&root_canon) {
                break;
            }
            curr = parent.parent();
        }

        self.refresh();

        // Select the newly created folder in items
        if let Some(pos) = self
            .items
            .iter()
            .position(|it| it.path == canon_target || it.path == normalized)
        {
            self.selected_idx = pos;
            if pos < self.scroll_offset {
                self.scroll_offset = pos;
            }
        }

        Ok(canon_target)
    }

    pub fn delete_folder<P: AsRef<Path>>(&mut self, path: P) -> std::io::Result<PathBuf> {
        let target = path.as_ref();
        let root_canon = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone());
        let target_canon = target.canonicalize()?;

        if target_canon == root_canon {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Cannot delete the workspace root directory",
            ));
        }

        if !target_canon.starts_with(&root_canon) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Cannot delete path outside workspace root",
            ));
        }

        if !target_canon.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Target is not a directory",
            ));
        }

        fs::remove_dir_all(&target_canon)?;

        // Prune deleted directory and all descendants from expanded_dirs
        self.expanded_dirs
            .retain(|p| !p.starts_with(&target_canon) && !p.starts_with(target));

        self.refresh();

        Ok(target_canon)
    }

    pub fn create_file<P: AsRef<Path>>(
        &mut self,
        base_dir: &Path,
        rel_path: P,
    ) -> std::io::Result<PathBuf> {
        let rel = rel_path.as_ref();
        let rel_str = rel.to_string_lossy();
        let trimmed_input = rel_str.trim();
        if trimmed_input.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "File name cannot be empty",
            ));
        }

        if trimmed_input.ends_with('/') || trimmed_input.ends_with('\\') {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "File name cannot end with a directory separator",
            ));
        }

        if rel.is_absolute()
            || trimmed_input.starts_with('/')
            || trimmed_input.starts_with('\\')
            || Path::new(trimmed_input).components().any(|c| {
                matches!(
                    c,
                    std::path::Component::Prefix(_) | std::path::Component::RootDir
                )
            })
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Absolute paths or drive prefixes are forbidden",
            ));
        }

        let root_canon = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone());
        if let Ok(canon_base) = base_dir.canonicalize() {
            if !canon_base.starts_with(&root_canon) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Base directory is outside workspace root",
                ));
            }
        }

        let mut depth_check = base_dir.to_path_buf();
        for comp in Path::new(trimmed_input).components() {
            match comp {
                std::path::Component::Prefix(_) | std::path::Component::RootDir => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "Absolute paths or drive prefixes are forbidden",
                    ));
                }
                std::path::Component::ParentDir => {
                    if !depth_check.pop() || !depth_check.starts_with(&root_canon) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            "Path traversal escaping workspace root is forbidden",
                        ));
                    }
                }
                std::path::Component::Normal(c) => depth_check.push(c),
                _ => {}
            }
        }

        let normalized = if base_dir.is_absolute() || base_dir.starts_with(&self.root) {
            base_dir.join(trimmed_input)
        } else {
            self.root.join(base_dir).join(trimmed_input)
        };

        if let Some(parent) = normalized.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        // Open with create_new to prevent accidental overwrite of existing files
        let mut open_opts = fs::OpenOptions::new();
        open_opts.write(true).create_new(true);
        open_opts.open(&normalized)?;

        let canon_target = normalized.canonicalize().unwrap_or(normalized.clone());
        if !canon_target.starts_with(&root_canon) {
            let _ = fs::remove_file(&canon_target);
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Target file resolved outside workspace root",
            ));
        }

        // Auto-expand all ancestor directories from root down to created file
        let mut curr = canon_target.parent();
        while let Some(parent) = curr {
            self.expanded_dirs.insert(parent.to_path_buf());
            if parent == root_canon || !parent.starts_with(&root_canon) {
                break;
            }
            curr = parent.parent();
        }

        self.refresh();

        // Select the newly created file in items
        if let Some(pos) = self
            .items
            .iter()
            .position(|it| it.path == canon_target || it.path == normalized)
        {
            self.selected_idx = pos;
            if pos < self.scroll_offset {
                self.scroll_offset = pos;
            }
        }

        Ok(canon_target)
    }

    pub fn rename_item<P: AsRef<Path>>(
        &mut self,
        source_path: &Path,
        new_name: P,
    ) -> std::io::Result<PathBuf> {
        let name_ref = new_name.as_ref();
        let name_str = name_ref.to_string_lossy();
        let trimmed_name = name_str.trim();
        if trimmed_name.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "New name cannot be empty",
            ));
        }

        if name_ref.is_absolute()
            || trimmed_name.starts_with('/')
            || trimmed_name.starts_with('\\')
            || Path::new(trimmed_name).components().any(|c| {
                matches!(
                    c,
                    std::path::Component::Prefix(_) | std::path::Component::RootDir
                )
            })
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Absolute paths or drive prefixes are forbidden",
            ));
        }

        let root_canon = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone());
        let source_canon = source_path.canonicalize()?;

        if source_canon == root_canon {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Cannot rename the workspace root directory",
            ));
        }

        if !source_canon.starts_with(&root_canon) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Source path is outside workspace root",
            ));
        }

        let parent_dir = source_canon.parent().unwrap_or(&root_canon);
        let mut depth_check = parent_dir.to_path_buf();
        for comp in Path::new(trimmed_name).components() {
            match comp {
                std::path::Component::Prefix(_) | std::path::Component::RootDir => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "Absolute paths or drive prefixes are forbidden",
                    ));
                }
                std::path::Component::ParentDir => {
                    if !depth_check.pop() || !depth_check.starts_with(&root_canon) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            "Path traversal escaping workspace root is forbidden",
                        ));
                    }
                }
                std::path::Component::Normal(c) => depth_check.push(c),
                _ => {}
            }
        }

        let target = parent_dir.join(trimmed_name);
        if target.exists() {
            if let Ok(canon_exist) = target.canonicalize() {
                if canon_exist != source_canon {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::AlreadyExists,
                        "Target path already exists",
                    ));
                }
            } else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "Target path already exists",
                ));
            }
        }

        let is_dir = source_canon.is_dir();
        fs::rename(&source_canon, &target)?;

        let target_canon = target.canonicalize().unwrap_or(target.clone());

        if is_dir {
            // Update expanded_dirs: replace source prefix with target prefix
            let mut updated_expanded = HashSet::new();
            for dir in &self.expanded_dirs {
                if dir == &source_canon || dir == source_path {
                    updated_expanded.insert(target_canon.clone());
                } else if let Ok(rel) = dir.strip_prefix(&source_canon) {
                    updated_expanded.insert(target_canon.join(rel));
                } else {
                    updated_expanded.insert(dir.clone());
                }
            }
            self.expanded_dirs = updated_expanded;
        }

        self.refresh();

        // Select the renamed item
        if let Some(pos) = self
            .items
            .iter()
            .position(|it| it.path == target_canon || it.path == target)
        {
            self.selected_idx = pos;
            if pos < self.scroll_offset {
                self.scroll_offset = pos;
            }
        }

        Ok(target_canon)
    }

    pub fn jump_to_prefix(&mut self, ch: char, max_rows: usize) -> bool {
        if self.items.is_empty() {
            return false;
        }
        let ch_lower = ch.to_ascii_lowercase();
        let total = self.items.len();
        for step in 1..=total {
            let idx = (self.selected_idx + step) % total;
            if let Some(first_char) = self.items[idx].name.chars().next() {
                if first_char.to_ascii_lowercase() == ch_lower {
                    self.selected_idx = idx;
                    if self.selected_idx < self.scroll_offset {
                        self.scroll_offset = self.selected_idx;
                    } else if max_rows > 0 && self.selected_idx >= self.scroll_offset + max_rows {
                        self.scroll_offset = self.selected_idx.saturating_sub(max_rows - 1);
                    }
                    return true;
                }
            }
        }
        false
    }
}
