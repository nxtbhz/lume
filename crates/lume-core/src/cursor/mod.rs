#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorAffinity {
    Upstream,
    Downstream,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub anchor: usize,
    pub head: usize,
    pub affinity: CursorAffinity,
}

impl Cursor {
    pub fn new(head: usize) -> Self {
        Self {
            anchor: head,
            head,
            affinity: CursorAffinity::Downstream,
        }
    }

    pub fn with_anchor(anchor: usize, head: usize) -> Self {
        Self {
            anchor,
            head,
            affinity: CursorAffinity::Downstream,
        }
    }

    #[inline]
    pub fn is_selection(&self) -> bool {
        self.anchor != self.head
    }

    #[inline]
    pub fn min(&self) -> usize {
        self.anchor.min(self.head)
    }

    #[inline]
    pub fn max(&self) -> usize {
        self.anchor.max(self.head)
    }

    #[inline]
    pub fn range(&self) -> std::ops::Range<usize> {
        self.min()..self.max()
    }

    pub fn move_to(&mut self, pos: usize) {
        self.anchor = pos;
        self.head = pos;
    }

    pub fn select_to(&mut self, pos: usize) {
        self.head = pos;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorSet {
    cursors: Vec<Cursor>,
    primary_idx: usize,
}

impl Default for CursorSet {
    fn default() -> Self {
        Self::new()
    }
}

impl CursorSet {
    pub fn new() -> Self {
        Self {
            cursors: vec![Cursor::new(0)],
            primary_idx: 0,
        }
    }

    pub fn single(head: usize) -> Self {
        Self {
            cursors: vec![Cursor::new(head)],
            primary_idx: 0,
        }
    }

    pub fn cursors(&self) -> &[Cursor] {
        &self.cursors
    }

    pub fn cursors_mut(&mut self) -> &mut [Cursor] {
        &mut self.cursors
    }

    pub fn primary(&self) -> &Cursor {
        &self.cursors[self.primary_idx]
    }

    pub fn primary_mut(&mut self) -> &mut Cursor {
        &mut self.cursors[self.primary_idx]
    }

    pub fn set_single(&mut self, head: usize) {
        self.cursors.clear();
        self.cursors.push(Cursor::new(head));
        self.primary_idx = 0;
    }

    pub fn set_cursors(&mut self, cursors: Vec<Cursor>) {
        if cursors.is_empty() {
            self.set_single(0);
        } else {
            self.cursors = cursors;
            self.primary_idx = 0;
            self.normalize();
        }
    }

    pub fn add_cursor(&mut self, cursor: Cursor) {
        self.cursors.push(cursor);
        self.normalize();
    }

    /// Sort cursors by position and merge overlapping selections (preserving adjacent ones)
    pub fn normalize(&mut self) {
        if self.cursors.is_empty() {
            self.primary_idx = 0;
            return;
        }
        if self.cursors.len() == 1 {
            self.primary_idx = 0;
            return;
        }

        let orig_primary = self.primary_idx.min(self.cursors.len() - 1);
        let mut tagged: Vec<(usize, Cursor)> = self.cursors.drain(..).enumerate().collect();
        tagged.sort_by_key(|(orig_idx, c)| (c.min(), c.max(), *orig_idx));

        let mut merged: Vec<Cursor> = Vec::with_capacity(tagged.len());
        let mut new_primary_idx = 0usize;

        for (orig_idx, cursor) in tagged {
            let is_primary = orig_idx == orig_primary;
            if let Some(last) = merged.last_mut() {
                let overlaps_strictly = cursor.min() < last.max();
                let same_point_caret = last.min() == cursor.min()
                    && (last.anchor == last.head || cursor.anchor == cursor.head);
                if overlaps_strictly || same_point_caret {
                    let new_min = last.min().min(cursor.min());
                    let new_max = last.max().max(cursor.max());
                    let forward = if last.anchor == last.head && cursor.is_selection() {
                        cursor.head >= cursor.anchor
                    } else {
                        last.head >= last.anchor
                    };
                    if forward {
                        last.anchor = new_min;
                        last.head = new_max;
                    } else {
                        last.anchor = new_max;
                        last.head = new_min;
                    }
                    if is_primary {
                        last.affinity = cursor.affinity;
                        new_primary_idx = merged.len() - 1;
                    }
                    continue;
                }
            }

            if is_primary {
                new_primary_idx = merged.len();
            }
            merged.push(cursor);
        }

        self.cursors = merged;
        self.primary_idx = new_primary_idx.min(self.cursors.len().saturating_sub(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_cursor() {
        let mut cursor = Cursor::new(10);
        assert_eq!(cursor.head, 10);
        assert_eq!(cursor.anchor, 10);
        assert!(!cursor.is_selection());

        cursor.select_to(20);
        assert!(cursor.is_selection());
        assert_eq!(cursor.min(), 10);
        assert_eq!(cursor.max(), 20);

        cursor.move_to(5);
        assert!(!cursor.is_selection());
        assert_eq!(cursor.head, 5);
    }

    #[test]
    fn test_cursor_set_normalize() {
        let mut set = CursorSet::new();
        // Add cursors that overlap: (0..5), (3..8), (12..15)
        set.add_cursor(Cursor::with_anchor(0, 5));
        set.add_cursor(Cursor::with_anchor(3, 8));
        set.add_cursor(Cursor::with_anchor(12, 15));

        // After normalization, (0..5) and (3..8) should merge into (0..8)
        let cursors = set.cursors();
        assert_eq!(cursors.len(), 2);
        assert_eq!(cursors[0].min(), 0);
        assert_eq!(cursors[0].max(), 8);
        assert_eq!(cursors[1].min(), 12);
        assert_eq!(cursors[1].max(), 15);
    }

    #[test]
    fn test_cursor_traits_and_constructors() {
        let def_set = CursorSet::default();
        assert_eq!(def_set.cursors().len(), 1);

        let single_set = CursorSet::single(42);
        assert_eq!(single_set.primary().head, 42);

        let c = Cursor::with_anchor(10, 20);
        assert_eq!(c.range(), 10..20);
    }

    #[test]
    fn test_cursor_normalize_preserves_adjacent_selections_and_primary() {
        // Adjacent non-empty selections [3, 6) (primary) and [0, 3) (secondary) must remain distinct,
        // and primary() must keep pointing to [3, 6) after sorting moves it to index 1.
        let mut set = CursorSet {
            cursors: vec![Cursor::with_anchor(3, 6)],
            primary_idx: 0,
        };
        set.add_cursor(Cursor::with_anchor(0, 3));
        assert_eq!(set.cursors().len(), 2);
        assert_eq!(set.cursors()[0].range(), 0..3);
        assert_eq!(set.cursors()[1].range(), 3..6);
        assert_eq!(set.primary().range(), 3..6);

        // Also check carets: adding a caret before the primary preserves primary().head == 5
        let mut caret_set = CursorSet::single(5);
        caret_set.add_cursor(Cursor::new(0));
        assert_eq!(caret_set.cursors().len(), 2);
        assert_eq!(caret_set.primary().head, 5);
    }
}
