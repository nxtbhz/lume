pub mod autosave;
pub mod buffer;
pub mod clipboard;
pub mod cursor;
pub mod editor;
pub mod fuzzy;
pub mod history;
pub mod journal;
pub mod search;

pub use autosave::{AutoSaveConfig, AutoSaveController};
pub use buffer::{BufferError, RopeBuffer, TextBuffer, TextDelta};
pub use clipboard::{
    encode_base64, osc52_copy_sequence, osc52_sequence_auto, osc52_sequence_with_passthrough,
};
pub use cursor::{Cursor, CursorAffinity, CursorSet};
pub use editor::{AsyncSaveEvent, Editor, EditorConfig, Tab};
pub use fuzzy::{fuzzy_match, fuzzy_score, FuzzyMatch};
pub use history::History;
pub use journal::Journal;
