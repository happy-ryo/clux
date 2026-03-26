use crate::pane::{PaneId, Rect};
use crate::tree::{Direction, LayoutNode};

pub struct Tab {
    pub name: String,
    root: LayoutNode,
    pub active_pane: PaneId,
    next_pane_id: PaneId,
}

impl Tab {
    /// Create a new tab with a single pane.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            root: LayoutNode::Leaf { pane_id: 0 },
            active_pane: 0,
            next_pane_id: 1,
        }
    }

    /// Split the active pane in the given direction. Returns the new pane ID.
    /// The new pane becomes active.
    pub fn split_active(&mut self, direction: Direction, ratio: f32) -> PaneId {
        let new_id = self.next_pane_id;
        self.next_pane_id += 1;
        self.root
            .split_pane(self.active_pane, new_id, direction, ratio);
        self.active_pane = new_id;
        new_id
    }

    /// Close a pane. If the closed pane was active, switch to the first
    /// available pane. Returns `true` if the pane was removed.
    pub fn close_pane(&mut self, pane_id: PaneId) -> bool {
        if !self.root.remove_pane(pane_id) {
            return false;
        }
        if self.active_pane == pane_id {
            // Switch to the first available pane.
            self.active_pane = self
                .root
                .all_pane_ids()
                .first()
                .copied()
                .expect("at least one pane must remain after removal");
        }
        true
    }

    /// Compute rects for all panes given a viewport.
    #[must_use]
    pub fn all_pane_rects(&self, viewport: Rect) -> Vec<(PaneId, Rect)> {
        self.root.compute_rects(viewport)
    }

    /// Return the number of panes.
    #[must_use]
    pub fn pane_count(&self) -> usize {
        self.root.pane_count()
    }

    /// Cycle focus to the next or previous pane in tree order.
    pub fn cycle_focus(&mut self, forward: bool) {
        let ids = self.root.all_pane_ids();
        if ids.is_empty() {
            return;
        }
        let current_idx = ids
            .iter()
            .position(|&id| id == self.active_pane)
            .unwrap_or(0);
        let next_idx = if forward {
            (current_idx + 1) % ids.len()
        } else {
            (current_idx + ids.len() - 1) % ids.len()
        };
        self.active_pane = ids[next_idx];
    }

    /// Set the active pane to whichever pane contains the given point.
    /// Returns `true` if focus changed.
    pub fn focus_at(&mut self, x: f32, y: f32, viewport: Rect) -> bool {
        for (id, rect) in self.root.compute_rects(viewport) {
            if rect.contains(x, y) {
                let changed = self.active_pane != id;
                self.active_pane = id;
                return changed;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> Rect {
        Rect::new(0.0, 0.0, 800.0, 600.0)
    }

    #[test]
    fn new_tab_has_one_pane() {
        let tab = Tab::new("test");
        assert_eq!(tab.pane_count(), 1);
        assert_eq!(tab.active_pane, 0);
    }

    #[test]
    fn split_active_increases_pane_count() {
        let mut tab = Tab::new("test");
        let new_id = tab.split_active(Direction::Vertical, 0.5);
        assert_eq!(tab.pane_count(), 2);
        assert_eq!(tab.active_pane, new_id);
    }

    #[test]
    fn close_pane_decreases_count() {
        let mut tab = Tab::new("test");
        tab.split_active(Direction::Vertical, 0.5);
        assert_eq!(tab.pane_count(), 2);
        assert!(tab.close_pane(1));
        assert_eq!(tab.pane_count(), 1);
    }

    #[test]
    fn close_pane_switches_active() {
        let mut tab = Tab::new("test");
        tab.split_active(Direction::Vertical, 0.5);
        // Active is pane 1 (the new pane).
        assert_eq!(tab.active_pane, 1);
        tab.close_pane(1);
        // Should switch to pane 0.
        assert_eq!(tab.active_pane, 0);
    }

    #[test]
    fn cycle_focus_wraps_around() {
        let mut tab = Tab::new("test");
        tab.split_active(Direction::Vertical, 0.5);
        tab.split_active(Direction::Horizontal, 0.5);
        // We have panes 0, 1, 2. Active is 2.
        tab.active_pane = 0;
        tab.cycle_focus(true);
        assert_eq!(tab.active_pane, 1);
        tab.cycle_focus(true);
        assert_eq!(tab.active_pane, 2);
        tab.cycle_focus(true);
        assert_eq!(tab.active_pane, 0); // wrapped around
        tab.cycle_focus(false);
        assert_eq!(tab.active_pane, 2); // backwards wrap
    }

    #[test]
    fn focus_at_selects_correct_pane() {
        let mut tab = Tab::new("test");
        tab.split_active(Direction::Vertical, 0.5);
        tab.active_pane = 0;
        // Click on right half should select pane 1.
        assert!(tab.focus_at(500.0, 300.0, viewport()));
        assert_eq!(tab.active_pane, 1);
        // Click on left half should select pane 0.
        assert!(tab.focus_at(100.0, 300.0, viewport()));
        assert_eq!(tab.active_pane, 0);
    }
}
