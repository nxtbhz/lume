use crossterm::{
    cursor::{MoveTo, Show},
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};
use lume_core::Cursor;
use std::io::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchMode {
    Find,
    Replace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchField {
    Query,
    Replacement,
}

#[derive(Clone, Debug)]
pub struct SearchState {
    pub mode: SearchMode,
    pub active_field: SearchField,
    pub query: String,
    pub query_cursor: usize,
    pub replacement: String,
    pub replacement_cursor: usize,
    pub initial_cursor: Cursor,
    pub initial_scroll: (usize, usize),
    pub match_count: usize,
    pub active_match_idx: usize,
}

impl SearchState {
    pub fn new_find(
        initial_cursor: Cursor,
        initial_scroll: (usize, usize),
        initial_query: Option<&str>,
    ) -> Self {
        let q = initial_query.unwrap_or("").to_string();
        let len = q.len();
        Self {
            mode: SearchMode::Find,
            active_field: SearchField::Query,
            query: q,
            query_cursor: len,
            replacement: String::new(),
            replacement_cursor: 0,
            initial_cursor,
            initial_scroll,
            match_count: 0,
            active_match_idx: 0,
        }
    }

    pub fn new_replace(
        initial_cursor: Cursor,
        initial_scroll: (usize, usize),
        initial_query: Option<&str>,
    ) -> Self {
        let q = initial_query.unwrap_or("").to_string();
        let len = q.len();
        let active_field = if q.is_empty() {
            SearchField::Query
        } else {
            SearchField::Replacement
        };
        Self {
            mode: SearchMode::Replace,
            active_field,
            query: q,
            query_cursor: len,
            replacement: String::new(),
            replacement_cursor: 0,
            initial_cursor,
            initial_scroll,
            match_count: 0,
            active_match_idx: 0,
        }
    }

    pub fn toggle_field(&mut self) {
        if self.mode == SearchMode::Replace {
            self.active_field = match self.active_field {
                SearchField::Query => SearchField::Replacement,
                SearchField::Replacement => SearchField::Query,
            };
        }
    }

    pub fn handle_char(&mut self, c: char) {
        match self.active_field {
            SearchField::Query => {
                self.query.insert(self.query_cursor, c);
                self.query_cursor += c.len_utf8();
            }
            SearchField::Replacement => {
                self.replacement.insert(self.replacement_cursor, c);
                self.replacement_cursor += c.len_utf8();
            }
        }
    }

    pub fn handle_backspace(&mut self) {
        match self.active_field {
            SearchField::Query => {
                if self.query_cursor > 0 {
                    if let Some(prev) = self.query[..self.query_cursor].chars().next_back() {
                        self.query_cursor -= prev.len_utf8();
                        self.query.remove(self.query_cursor);
                    }
                }
            }
            SearchField::Replacement => {
                if self.replacement_cursor > 0 {
                    if let Some(prev) = self.replacement[..self.replacement_cursor]
                        .chars()
                        .next_back()
                    {
                        self.replacement_cursor -= prev.len_utf8();
                        self.replacement.remove(self.replacement_cursor);
                    }
                }
            }
        }
    }

    pub fn handle_delete(&mut self) {
        match self.active_field {
            SearchField::Query => {
                if self.query_cursor < self.query.len() {
                    self.query.remove(self.query_cursor);
                }
            }
            SearchField::Replacement => {
                if self.replacement_cursor < self.replacement.len() {
                    self.replacement.remove(self.replacement_cursor);
                }
            }
        }
    }

    pub fn move_left(&mut self) {
        match self.active_field {
            SearchField::Query => {
                if self.query_cursor > 0 {
                    if let Some(prev) = self.query[..self.query_cursor].chars().next_back() {
                        self.query_cursor -= prev.len_utf8();
                    }
                }
            }
            SearchField::Replacement => {
                if self.replacement_cursor > 0 {
                    if let Some(prev) = self.replacement[..self.replacement_cursor]
                        .chars()
                        .next_back()
                    {
                        self.replacement_cursor -= prev.len_utf8();
                    }
                }
            }
        }
    }

    pub fn move_right(&mut self) {
        match self.active_field {
            SearchField::Query => {
                if self.query_cursor < self.query.len() {
                    if let Some(next) = self.query[self.query_cursor..].chars().next() {
                        self.query_cursor += next.len_utf8();
                    }
                }
            }
            SearchField::Replacement => {
                if self.replacement_cursor < self.replacement.len() {
                    if let Some(next) = self.replacement[self.replacement_cursor..].chars().next() {
                        self.replacement_cursor += next.len_utf8();
                    }
                }
            }
        }
    }

    pub fn move_home(&mut self) {
        match self.active_field {
            SearchField::Query => self.query_cursor = 0,
            SearchField::Replacement => self.replacement_cursor = 0,
        }
    }

    pub fn move_end(&mut self) {
        match self.active_field {
            SearchField::Query => self.query_cursor = self.query.len(),
            SearchField::Replacement => self.replacement_cursor = self.replacement.len(),
        }
    }

    pub fn render<W: Write>(
        &self,
        writer: &mut W,
        terminal_width: u16,
        row: u16,
    ) -> std::io::Result<()> {
        let width = terminal_width as usize;
        let bg_color = Color::Rgb {
            r: 24,
            g: 28,
            b: 38,
        };

        queue!(
            writer,
            MoveTo(0, row),
            SetBackgroundColor(bg_color),
            SetForegroundColor(Color::White)
        )?;

        let cursor_col: usize;

        match self.mode {
            SearchMode::Find => {
                let prefix = " Find: ";
                let query_display = &self.query;
                let status_text = if self.query.is_empty() {
                    " (Type to search) ".to_string()
                } else if self.match_count == 0 {
                    " (No matches) ".to_string()
                } else {
                    format!(" [{}/{}] ", self.active_match_idx + 1, self.match_count)
                };
                let hints = " (Enter/F3: Next, Shift+Enter/Shift+F3: Prev, Esc: Cancel) ";

                cursor_col =
                    prefix.chars().count() + self.query[..self.query_cursor].chars().count();

                let left_content = format!("{}{}{}", prefix, query_display, status_text);
                let spaces =
                    width.saturating_sub(left_content.chars().count() + hints.chars().count());
                let full_bar = format!("{}{:spaces$}{}", left_content, "", hints, spaces = spaces);
                let trimmed: String = full_bar.chars().take(width).collect();

                queue!(writer, Print(trimmed), ResetColor)?;
            }
            SearchMode::Replace => {
                let find_prefix = if self.active_field == SearchField::Query {
                    " Find*: "
                } else {
                    " Find: "
                };
                let replace_prefix = if self.active_field == SearchField::Replacement {
                    " │ Substituir por*: "
                } else {
                    " │ Substituir por: "
                };

                let status_text = if self.query.is_empty() {
                    "".to_string()
                } else if self.match_count == 0 {
                    " [0 matches] ".to_string()
                } else {
                    format!(" [{}/{}] ", self.active_match_idx + 1, self.match_count)
                };

                let hints = " (Enter: Replace, Ctrl+Alt+Enter: All, Tab: Field, Esc: Cancel) ";

                if self.active_field == SearchField::Query {
                    cursor_col = find_prefix.chars().count()
                        + self.query[..self.query_cursor].chars().count();
                } else {
                    cursor_col = find_prefix.chars().count()
                        + self.query.chars().count()
                        + replace_prefix.chars().count()
                        + self.replacement[..self.replacement_cursor].chars().count();
                }

                let left_content = format!(
                    "{}{}{}{}{}",
                    find_prefix, self.query, replace_prefix, self.replacement, status_text
                );
                let spaces =
                    width.saturating_sub(left_content.chars().count() + hints.chars().count());
                let full_bar = format!("{}{:spaces$}{}", left_content, "", hints, spaces = spaces);
                let trimmed: String = full_bar.chars().take(width).collect();

                queue!(writer, Print(trimmed), ResetColor)?;
            }
        }

        // Position terminal cursor in active field
        let safe_col = (cursor_col as u16).min(terminal_width.saturating_sub(1));
        queue!(writer, MoveTo(safe_col, row), Show)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lume_core::Cursor;

    #[test]
    fn test_search_state_find_editing() {
        let mut state = SearchState::new_find(Cursor::new(10), (2, 0), None);
        assert_eq!(state.mode, SearchMode::Find);
        assert_eq!(state.active_field, SearchField::Query);

        for c in "foo_ação".chars() {
            state.handle_char(c);
        }
        assert_eq!(state.query, "foo_ação");
        assert_eq!(state.query_cursor, state.query.len());

        state.move_left(); // before 'o'
        state.move_left(); // before 'ã'
        state.handle_backspace(); // removes 'ç'
        assert_eq!(state.query, "foo_aão");
    }

    #[test]
    fn test_search_state_replace_toggle() {
        let mut state = SearchState::new_replace(Cursor::new(0), (0, 0), Some("init_query"));
        assert_eq!(state.mode, SearchMode::Replace);
        assert_eq!(state.query, "init_query");
        assert_eq!(state.active_field, SearchField::Replacement);

        state.toggle_field();
        assert_eq!(state.active_field, SearchField::Query);
        state.toggle_field();
        assert_eq!(state.active_field, SearchField::Replacement);
    }

    #[test]
    fn test_search_state_replacement_field_operations() {
        let mut state = SearchState::new_replace(Cursor::new(0), (0, 0), Some("target"));
        assert_eq!(state.active_field, SearchField::Replacement);
        state.handle_char('n');
        state.handle_char('e');
        state.handle_char('w');
        assert_eq!(state.replacement, "new");
        assert_eq!(state.replacement_cursor, 3);

        // Left, Right, Home, End
        state.move_left();
        assert_eq!(state.replacement_cursor, 2);
        state.move_home();
        assert_eq!(state.replacement_cursor, 0);
        state.move_end();
        assert_eq!(state.replacement_cursor, 3);
        state.move_right();
        assert_eq!(state.replacement_cursor, 3);

        // Backspace & Delete
        state.handle_backspace();
        assert_eq!(state.replacement, "ne");
        state.move_home();
        state.handle_delete();
        assert_eq!(state.replacement, "e");
    }

    #[test]
    fn test_search_state_query_field_operations() {
        let mut state = SearchState::new_find(Cursor::new(0), (0, 0), None);
        state.handle_char('h');
        state.handle_char('i');
        state.move_left();
        state.handle_delete();
        assert_eq!(state.query, "h");
        state.move_home();
        assert_eq!(state.query_cursor, 0);
        state.move_end();
        assert_eq!(state.query_cursor, 1);
        state.move_right();
        assert_eq!(state.query_cursor, 1);
    }

    #[test]
    fn test_search_state_render_find_and_replace_modes() {
        let mut buf = Vec::new();
        let mut state = SearchState::new_find(Cursor::new(0), (0, 0), Some("query"));
        state.match_count = 3;
        state.active_match_idx = 1;
        assert!(state.render(&mut buf, 80, 24).is_ok());
        assert!(!buf.is_empty());

        // Empty query find
        buf.clear();
        let empty_state = SearchState::new_find(Cursor::new(0), (0, 0), None);
        assert!(empty_state.render(&mut buf, 80, 24).is_ok());

        // Zero matches find
        buf.clear();
        let mut zero_state = SearchState::new_find(Cursor::new(0), (0, 0), Some("notfound"));
        zero_state.match_count = 0;
        assert!(zero_state.render(&mut buf, 80, 24).is_ok());

        // Replace mode with query active & replacement active
        buf.clear();
        let mut replace_state = SearchState::new_replace(Cursor::new(0), (0, 0), Some("find_me"));
        replace_state.replacement = "replace_me".to_string();
        replace_state.match_count = 2;
        assert!(replace_state.render(&mut buf, 80, 24).is_ok());
        replace_state.toggle_field();
        buf.clear();
        assert!(replace_state.render(&mut buf, 80, 24).is_ok());
    }
}
