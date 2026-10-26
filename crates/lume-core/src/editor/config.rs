#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorConfig {
    pub smart_indent: bool,
    pub smart_unindent: bool,
    pub auto_pairs: bool,
    pub tab_size: usize,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            smart_indent: true,
            smart_unindent: true,
            auto_pairs: true,
            tab_size: 4,
        }
    }
}
