use crate::buffer::{TextBuffer, TextDelta};
use crate::editor::Editor;
use crate::history::HistoryManager;

impl Editor {
    pub fn find_next(&mut self, query: &str, case_sensitive: bool) -> Option<(usize, usize)> {
        if query.is_empty() {
            return None;
        }
        let tab = &mut self.tabs[self.active_tab];
        let from_byte = if tab.cursors.primary().is_selection() {
            tab.cursors.primary().max()
        } else {
            tab.cursors.primary().head
        };
        if let Some((start, end)) =
            crate::search::find_next(&tab.buffer, query, from_byte, case_sensitive)
        {
            let primary = tab.cursors.primary_mut();
            primary.anchor = start;
            primary.head = end;
            Some((start, end))
        } else {
            None
        }
    }

    pub fn find_prev(&mut self, query: &str, case_sensitive: bool) -> Option<(usize, usize)> {
        if query.is_empty() {
            return None;
        }
        let tab = &mut self.tabs[self.active_tab];
        let from_byte = if tab.cursors.primary().is_selection() {
            tab.cursors.primary().min()
        } else {
            tab.cursors.primary().head
        };
        if let Some((start, end)) =
            crate::search::find_prev(&tab.buffer, query, from_byte, case_sensitive)
        {
            let primary = tab.cursors.primary_mut();
            primary.anchor = start;
            primary.head = end;
            Some((start, end))
        } else {
            None
        }
    }

    pub fn replace_current(
        &mut self,
        query: &str,
        replacement: &str,
        case_sensitive: bool,
    ) -> bool {
        if query.is_empty() {
            return false;
        }

        let tab = &self.tabs[self.active_tab];
        let primary = *tab.cursors.primary();
        let sel_min = primary.min();
        let sel_max = primary.max();

        let is_match_selected = if primary.is_selection() {
            let selected_text = tab.buffer.slice_to_cow(sel_min, sel_max);
            if case_sensitive {
                selected_text == query
            } else {
                selected_text.eq_ignore_ascii_case(query)
                    || selected_text.to_lowercase() == query.to_lowercase()
            }
        } else {
            false
        };

        if !is_match_selected {
            // Preview: select next match if available, without modifying buffer
            return self.find_next(query, case_sensitive).is_some() && false;
        }

        let target_start = sel_min;
        let target_end = sel_max;

        let tab = &mut self.tabs[self.active_tab];
        let cursors_before = tab.cursors.cursors().to_vec();

        let old_text = match tab.buffer.delete(target_start, target_end) {
            Ok(delta) => delta.old_text,
            Err(_) => return false,
        };

        if tab.buffer.insert(target_start, replacement).is_err() {
            return false;
        }

        let new_pos = target_start + replacement.len();
        tab.cursors.set_single(new_pos);

        let delta = TextDelta::new(target_start, old_text, replacement);
        let deltas = vec![delta];
        let cursors_after = tab.cursors.cursors().to_vec();

        tab.history
            .record_edits(deltas.clone(), cursors_before, cursors_after);
        self.record_edit_internal(&deltas);

        // Advance to next match after this replacement
        let _ = self.find_next(query, case_sensitive);

        true
    }

    pub fn replace_all(&mut self, query: &str, replacement: &str, case_sensitive: bool) -> usize {
        if query.is_empty() {
            return 0;
        }

        let tab = &self.tabs[self.active_tab];
        let total_bytes = tab.buffer.len_bytes();
        if total_bytes == 0 {
            return 0;
        }

        let matches = crate::search::find_matches_in_range(
            &tab.buffer,
            query,
            0,
            total_bytes,
            case_sensitive,
        );
        if matches.is_empty() {
            return 0;
        }

        let count = matches.len();
        let tab = &mut self.tabs[self.active_tab];
        let cursors_before = tab.cursors.cursors().to_vec();

        let mut deltas = Vec::with_capacity(count);
        let mut delta_offset: isize = 0;
        let mut last_end = 0;

        for (orig_start, orig_end) in matches {
            let start = (orig_start as isize + delta_offset) as usize;
            let end = (orig_end as isize + delta_offset) as usize;

            if let Ok(del_delta) = tab.buffer.delete(start, end) {
                if let Ok(ins_delta) = tab.buffer.insert(start, replacement) {
                    last_end = start + ins_delta.new_text.len();
                    deltas.push(TextDelta::new(
                        start,
                        del_delta.old_text,
                        ins_delta.new_text,
                    ));
                    delta_offset += replacement.len() as isize - (orig_end - orig_start) as isize;
                }
            }
        }

        if deltas.is_empty() {
            return 0;
        }

        tab.cursors.set_single(last_end);
        let cursors_after = tab.cursors.cursors().to_vec();

        tab.history
            .record_edits(deltas.clone(), cursors_before, cursors_after);
        self.record_edit_internal(&deltas);

        count
    }
}
