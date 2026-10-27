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
pub mod mouse_tests;
pub mod palette_tests;
pub mod vim_tests;
