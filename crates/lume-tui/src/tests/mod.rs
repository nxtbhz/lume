use std::path::{Path, PathBuf};

pub struct TestDir(pub PathBuf);

impl TestDir {
    pub fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "{}_{}_{:?}",
            prefix,
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        let path = path.canonicalize().unwrap_or(path);
        TestDir(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub mod app_tests;
pub mod clipboard_tests;
pub mod explorer_tests;
pub mod goto_line_tests;
pub mod mouse_tests;
pub mod palette_tests;
pub mod quick_open_tests;
pub mod quick_open_tui_tests;
pub mod search_tests;
pub mod vim_tests;
