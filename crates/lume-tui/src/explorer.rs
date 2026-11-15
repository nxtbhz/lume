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

    fn validate_existing_ancestor_chain(
        root_raw: &Path,
        root_canon: &Path,
        existing_path: &Path,
    ) -> std::io::Result<PathBuf> {
        let mut clean = PathBuf::new();
        for comp in existing_path.components() {
            match comp {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "Path traversal in existing path is forbidden",
                    ));
                }
                _ => clean.push(comp),
            }
        }
        if clean.as_os_str().is_empty() {
            clean.push(".");
        }

        let meta = fs::symlink_metadata(&clean)?;
        if meta.file_type().is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Symlinks are forbidden in target path",
            ));
        }

        let canon = clean.canonicalize()?;
        if !canon.starts_with(root_canon) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Path is outside workspace root",
            ));
        }

        // Walk lexically from `clean` upward without per-step `canonicalize()` so
        // `ws/loop -> ws` loops are never skipped, while still stopping at uncanonical `root_raw`.
        let mut cur = clean;
        while cur != *root_canon && cur != *root_raw {
            let m = fs::symlink_metadata(&cur)?;
            if m.file_type().is_symlink() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Symlink in intermediate path is forbidden",
                ));
            }
            match cur.parent() {
                Some(parent) if parent != cur && !parent.as_os_str().is_empty() => {
                    cur = parent.to_path_buf();
                }
                _ => break,
            }
        }

        Ok(canon)
    }

    fn resolve_safe_target(
        &self,
        root_canon: &Path,
        base_dir: &Path,
        rel_path: &Path,
    ) -> std::io::Result<PathBuf> {
        let rel_str = rel_path.to_string_lossy();
        let trimmed = rel_str.trim();
        if trimmed.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Target name cannot be empty",
            ));
        }

        if rel_path.is_absolute()
            || trimmed.starts_with('/')
            || trimmed.starts_with('\\')
            || Path::new(trimmed).components().any(|c| {
                matches!(
                    c,
                    std::path::Component::Prefix(_)
                        | std::path::Component::RootDir
                        | std::path::Component::ParentDir
                )
            })
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Absolute paths, drive prefixes, and parent directory traversal are forbidden",
            ));
        }

        if base_dir
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Path traversal in base directory is forbidden",
            ));
        }

        let raw_base = if base_dir.is_absolute() || base_dir.starts_with(&self.root) {
            base_dir.to_path_buf()
        } else {
            self.root.join(base_dir)
        };

        // Walk upward via symlink_metadata so dangling symlinks are inspected rather than skipped
        let mut cursor = raw_base;
        let mut missing_base_tail = Vec::new();
        let existing_canon = loop {
            match fs::symlink_metadata(&cursor) {
                Ok(_) => {
                    break Self::validate_existing_ancestor_chain(&self.root, root_canon, &cursor)?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    let Some(name) = cursor.file_name() else {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            "Base directory is outside workspace root",
                        ));
                    };
                    missing_base_tail.push(name.to_os_string());
                    let Some(parent) = cursor.parent() else {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            "Base directory is outside workspace root",
                        ));
                    };
                    cursor = parent.to_path_buf();
                }
                Err(e) => return Err(e),
            }
        };

        if !existing_canon.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Base path is not a directory",
            ));
        }

        let mut target = existing_canon;
        for seg in missing_base_tail.into_iter().rev() {
            target.push(seg);
        }

        let mut pushed_normal = false;
        for comp in Path::new(trimmed).components() {
            match comp {
                std::path::Component::Normal(seg) => {
                    pushed_normal = true;
                    target.push(seg);
                    match fs::symlink_metadata(&target) {
                        Ok(meta) => {
                            if meta.file_type().is_symlink() {
                                return Err(std::io::Error::new(
                                    std::io::ErrorKind::PermissionDenied,
                                    "Symlinks are forbidden in target path",
                                ));
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => return Err(e),
                    }
                }
                std::path::Component::CurDir => {}
                std::path::Component::Prefix(_)
                | std::path::Component::RootDir
                | std::path::Component::ParentDir => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "Absolute paths, drive prefixes, and parent directory traversal are forbidden",
                    ));
                }
            }
        }

        if !pushed_normal {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Target name cannot be empty",
            ));
        }

        if !target.starts_with(root_canon) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Target path resolved outside workspace root",
            ));
        }

        Ok(target)
    }

    fn ensure_dir_no_symlinks(root_canon: &Path, target_dir: &Path) -> std::io::Result<()> {
        let rel = target_dir.strip_prefix(root_canon).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Target directory resolved outside workspace root",
            )
        })?;
        let mut curr = root_canon.to_path_buf();
        for comp in rel.components() {
            curr.push(comp);
            match fs::create_dir(&curr) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let meta = fs::symlink_metadata(&curr)?;
                    if meta.file_type().is_symlink() || !meta.is_dir() {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            "Intermediate path component is a symlink or non-directory",
                        ));
                    }
                }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    fn expand_ancestors_to_root(&mut self, root_canon: &Path, target: &Path) {
        let mut curr = target.parent();
        while let Some(parent) = curr {
            self.expanded_dirs.insert(parent.to_path_buf());
            if parent == root_canon || !parent.starts_with(root_canon) {
                break;
            }
            curr = parent.parent();
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

        if rel.is_absolute() || trimmed_input.starts_with('/') || trimmed_input.starts_with('\\') {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Absolute paths or drive prefixes are forbidden",
            ));
        }

        let trimmed = trimmed_input.trim_end_matches(['/', '\\']);
        let root_canon = self.root.canonicalize()?;
        let target = self.resolve_safe_target(&root_canon, base_dir, Path::new(trimmed))?;

        Self::ensure_dir_no_symlinks(&root_canon, &target)?;
        self.expand_ancestors_to_root(&root_canon, &target);

        self.refresh();

        // Select the newly created folder in items
        if let Some(pos) = self.items.iter().position(|it| it.path == target) {
            self.selected_idx = pos;
            if pos < self.scroll_offset {
                self.scroll_offset = pos;
            }
        }

        Ok(target)
    }

    pub fn delete_folder<P: AsRef<Path>>(&mut self, path: P) -> std::io::Result<PathBuf> {
        let target = path.as_ref();
        let root_canon = self.root.canonicalize()?;
        let raw_target = if target.is_absolute() || target.starts_with(&self.root) {
            target.to_path_buf()
        } else {
            self.root.join(target)
        };
        let target_canon =
            Self::validate_existing_ancestor_chain(&self.root, &root_canon, &raw_target)?;

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

        let root_canon = self.root.canonicalize()?;
        let target = self.resolve_safe_target(&root_canon, base_dir, rel)?;

        if let Some(parent) = target.parent() {
            Self::ensure_dir_no_symlinks(&root_canon, parent)?;
        }

        // Open with create_new to prevent accidental overwrite of existing files or symlinks
        let mut open_opts = fs::OpenOptions::new();
        open_opts.write(true).create_new(true);
        open_opts.open(&target)?;

        self.expand_ancestors_to_root(&root_canon, &target);
        self.refresh();

        // Select the newly created file in items
        if let Some(pos) = self.items.iter().position(|it| it.path == target) {
            self.selected_idx = pos;
            if pos < self.scroll_offset {
                self.scroll_offset = pos;
            }
        }

        Ok(target)
    }

    pub fn rename_item<P: AsRef<Path>>(
        &mut self,
        source_path: &Path,
        new_name: P,
    ) -> std::io::Result<PathBuf> {
        let name_ref = new_name.as_ref();
        let root_canon = self.root.canonicalize()?;
        let source_canon =
            Self::validate_existing_ancestor_chain(&self.root, &root_canon, source_path)?;

        if source_canon == root_canon {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Cannot rename the workspace root directory",
            ));
        }

        let parent_dir = source_canon.parent().unwrap_or(&root_canon);
        let target = self.resolve_safe_target(&root_canon, parent_dir, name_ref)?;

        if let Some(dest_parent) = target.parent() {
            if fs::symlink_metadata(dest_parent).is_ok() {
                Self::validate_existing_ancestor_chain(&self.root, &root_canon, dest_parent)?;
            }
        }

        match fs::symlink_metadata(&target) {
            Ok(_) => {
                if target.canonicalize().ok().as_ref() != Some(&source_canon) {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::AlreadyExists,
                        "Target path already exists",
                    ));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }

        let is_dir = source_canon.is_dir();
        fs::rename(&source_canon, &target)?;

        if is_dir {
            // Update expanded_dirs: replace source prefix with target prefix
            let mut updated_expanded = HashSet::new();
            for dir in &self.expanded_dirs {
                if dir == &source_canon || dir == source_path {
                    updated_expanded.insert(target.clone());
                } else if let Ok(rel) = dir.strip_prefix(&source_canon) {
                    updated_expanded.insert(target.join(rel));
                } else if let Ok(rel) = dir.strip_prefix(source_path) {
                    updated_expanded.insert(target.join(rel));
                } else {
                    updated_expanded.insert(dir.clone());
                }
            }
            self.expanded_dirs = updated_expanded;
        }

        self.refresh();

        // Select the renamed item
        if let Some(pos) = self.items.iter().position(|it| it.path == target) {
            self.selected_idx = pos;
            if pos < self.scroll_offset {
                self.scroll_offset = pos;
            }
        }

        Ok(target)
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
