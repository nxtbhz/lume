use lume_core::CursorSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitDirection {
    Vertical,   // Side-by-side (|)
    Horizontal, // Stacked (-)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pane {
    pub tab_idx: usize,
    pub scroll_row: usize,
    pub scroll_col: usize,
    pub cursors: CursorSet,
}

impl Pane {
    pub fn new(tab_idx: usize, scroll_row: usize, scroll_col: usize, cursors: CursorSet) -> Self {
        Self {
            tab_idx,
            scroll_row,
            scroll_col,
            cursors,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneRect {
    pub left: usize,
    pub top: usize,
    pub width: usize,
    pub height: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitTarget {
    Pane(usize),
    Separator,
    Outside,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitState {
    pub is_split: bool,
    pub direction: SplitDirection,
    pub active_pane: usize,
    pub panes: [Pane; 2],
}

impl Default for SplitState {
    fn default() -> Self {
        Self::new()
    }
}

impl SplitState {
    pub fn new() -> Self {
        Self {
            is_split: false,
            direction: SplitDirection::Vertical,
            active_pane: 0,
            panes: [
                Pane::new(0, 0, 0, CursorSet::new()),
                Pane::new(0, 0, 0, CursorSet::new()),
            ],
        }
    }

    pub fn split_vertical(&mut self, tab_idx: usize, scroll: (usize, usize), cursors: CursorSet) {
        if !self.is_split {
            let pane = Pane::new(tab_idx, scroll.0, scroll.1, cursors);
            self.panes = [pane.clone(), pane];
            self.is_split = true;
            self.direction = SplitDirection::Vertical;
            self.active_pane = 1;
        } else {
            self.direction = SplitDirection::Vertical;
        }
    }

    pub fn split_horizontal(&mut self, tab_idx: usize, scroll: (usize, usize), cursors: CursorSet) {
        if !self.is_split {
            let pane = Pane::new(tab_idx, scroll.0, scroll.1, cursors);
            self.panes = [pane.clone(), pane];
            self.is_split = true;
            self.direction = SplitDirection::Horizontal;
            self.active_pane = 1;
        } else {
            self.direction = SplitDirection::Horizontal;
        }
    }

    #[cfg(test)]
    pub fn toggle_focus(&mut self) {
        if self.is_split {
            self.active_pane = if self.active_pane == 0 { 1 } else { 0 };
        }
    }

    pub fn close_active(&mut self) {
        if self.is_split {
            if self.active_pane == 0 {
                self.panes[0] = self.panes[1].clone();
            }
            self.is_split = false;
            self.active_pane = 0;
        }
    }

    pub fn layout(
        &self,
        left: usize,
        top: usize,
        width: usize,
        height: usize,
    ) -> (PaneRect, Option<PaneRect>, Option<PaneRect>) {
        if !self.is_split {
            return (
                PaneRect {
                    left,
                    top,
                    width,
                    height,
                },
                None,
                None,
            );
        }

        match self.direction {
            SplitDirection::Vertical => {
                let avail_w = width.saturating_sub(1);
                let w0 = avail_w / 2;
                let w1 = avail_w.saturating_sub(w0);
                let sep_left = left + w0;
                let sep_w = width.saturating_sub(avail_w);
                let p1_left = sep_left + sep_w;

                let p0 = PaneRect {
                    left,
                    top,
                    width: w0,
                    height,
                };
                let sep = Some(PaneRect {
                    left: sep_left,
                    top,
                    width: sep_w,
                    height,
                });
                let p1 = Some(PaneRect {
                    left: p1_left,
                    top,
                    width: w1,
                    height,
                });
                (p0, sep, p1)
            }
            SplitDirection::Horizontal => {
                let avail_h = height.saturating_sub(1);
                let h0 = avail_h / 2;
                let h1 = avail_h.saturating_sub(h0);
                let sep_top = top + h0;
                let sep_h = height.saturating_sub(avail_h);
                let p1_top = sep_top + sep_h;

                let p0 = PaneRect {
                    left,
                    top,
                    width,
                    height: h0,
                };
                let sep = Some(PaneRect {
                    left,
                    top: sep_top,
                    width,
                    height: sep_h,
                });
                let p1 = Some(PaneRect {
                    left,
                    top: p1_top,
                    width,
                    height: h1,
                });
                (p0, sep, p1)
            }
        }
    }

    pub fn hit_test(
        &self,
        col: usize,
        row: usize,
        left: usize,
        top: usize,
        width: usize,
        height: usize,
    ) -> HitTarget {
        if col < left || col >= left + width || row < top || row >= top + height {
            return HitTarget::Outside;
        }

        if !self.is_split {
            return HitTarget::Pane(0);
        }

        let (p0, sep, p1) = self.layout(left, top, width, height);
        if col >= p0.left && col < p0.left + p0.width && row >= p0.top && row < p0.top + p0.height {
            return HitTarget::Pane(0);
        }
        if let Some(s) = sep {
            if col >= s.left && col < s.left + s.width && row >= s.top && row < s.top + s.height {
                return HitTarget::Separator;
            }
        }
        if let Some(p1) = p1 {
            if col >= p1.left
                && col < p1.left + p1.width
                && row >= p1.top
                && row < p1.top + p1.height
            {
                return HitTarget::Pane(1);
            }
        }
        HitTarget::Outside
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lume_core::CursorSet;

    #[test]
    fn test_split_state_default() {
        let state = SplitState::default();
        assert!(!state.is_split);
        assert_eq!(state.active_pane, 0);
        assert_eq!(state.direction, SplitDirection::Vertical);
        let (p0, sep, p1) = state.layout(0, 1, 80, 20);
        assert_eq!(
            p0,
            PaneRect {
                left: 0,
                top: 1,
                width: 80,
                height: 20
            }
        );
        assert!(sep.is_none());
        assert!(p1.is_none());
        assert_eq!(state.hit_test(10, 5, 0, 1, 80, 20), HitTarget::Pane(0));
    }

    #[test]
    fn test_vertical_split_rects() {
        let mut state = SplitState::new();
        state.split_vertical(0, (0, 0), CursorSet::new());
        assert!(state.is_split);
        assert_eq!(state.direction, SplitDirection::Vertical);

        // Terminal text area: left=10, top=1, width=80, height=20
        let (p0, sep, p1) = state.layout(10, 1, 80, 20);
        // 80 - 1 = 79 avail; w0 = 39, w1 = 40
        assert_eq!(
            p0,
            PaneRect {
                left: 10,
                top: 1,
                width: 39,
                height: 20
            }
        );
        assert_eq!(
            sep,
            Some(PaneRect {
                left: 49,
                top: 1,
                width: 1,
                height: 20
            })
        );
        assert_eq!(
            p1,
            Some(PaneRect {
                left: 50,
                top: 1,
                width: 40,
                height: 20
            })
        );
    }

    #[test]
    fn test_horizontal_split_rects() {
        let mut state = SplitState::new();
        state.split_horizontal(0, (0, 0), CursorSet::new());
        assert!(state.is_split);
        assert_eq!(state.direction, SplitDirection::Horizontal);

        // Terminal text area: left=0, top=1, width=80, height=21
        let (p0, sep, p1) = state.layout(0, 1, 80, 21);
        // 21 - 1 = 20 avail; h0 = 10, h1 = 10
        assert_eq!(
            p0,
            PaneRect {
                left: 0,
                top: 1,
                width: 80,
                height: 10
            }
        );
        assert_eq!(
            sep,
            Some(PaneRect {
                left: 0,
                top: 11,
                width: 80,
                height: 1
            })
        );
        assert_eq!(
            p1,
            Some(PaneRect {
                left: 0,
                top: 12,
                width: 80,
                height: 10
            })
        );
    }

    #[test]
    fn test_hit_test_panes_and_separator() {
        let mut state = SplitState::new();
        state.split_vertical(0, (0, 0), CursorSet::new());

        // Area: left=10, top=1, width=80, height=20 -> p0: 10..49, sep: 49, p1: 50..90
        assert_eq!(state.hit_test(15, 5, 10, 1, 80, 20), HitTarget::Pane(0));
        assert_eq!(state.hit_test(49, 5, 10, 1, 80, 20), HitTarget::Separator);
        assert_eq!(state.hit_test(60, 5, 10, 1, 80, 20), HitTarget::Pane(1));
        assert_eq!(state.hit_test(5, 5, 10, 1, 80, 20), HitTarget::Outside);

        // Switch to horizontal and test hit_test
        state.split_horizontal(0, (0, 0), CursorSet::new());
        assert_eq!(state.hit_test(15, 5, 0, 1, 80, 21), HitTarget::Pane(0));
        assert_eq!(state.hit_test(15, 11, 0, 1, 80, 21), HitTarget::Separator);
        assert_eq!(state.hit_test(15, 15, 0, 1, 80, 21), HitTarget::Pane(1));
    }

    #[test]
    fn test_narrow_terminal_geometry_no_panic() {
        let mut state = SplitState::new();
        state.split_vertical(0, (0, 0), CursorSet::new());
        let (p0, _sep, p1) = state.layout(0, 0, 1, 1);
        assert_eq!(p0.width, 0);
        assert_eq!(p0.height, 1);
        assert!(p1.is_some());
    }

    #[test]
    fn test_toggle_focus_and_close() {
        let mut state = SplitState::new();
        state.split_vertical(0, (0, 0), CursorSet::new());
        assert_eq!(state.active_pane, 1);
        state.toggle_focus();
        assert_eq!(state.active_pane, 0);
        state.toggle_focus();
        assert_eq!(state.active_pane, 1);

        state.close_active();
        assert!(!state.is_split);
        assert_eq!(state.active_pane, 0);

        // Also test closing when active_pane == 0 preserves pane 1's state in pane 0
        state.split_vertical(0, (0, 0), CursorSet::new());
        state.panes[1].tab_idx = 3;
        state.toggle_focus();
        assert_eq!(state.active_pane, 0);
        state.close_active();
        assert!(!state.is_split);
        assert_eq!(state.panes[0].tab_idx, 3);
    }
}
