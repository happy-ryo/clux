use crate::pane::{PaneId, Rect};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    /// Split top/bottom.
    Horizontal,
    /// Split left/right.
    Vertical,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum LayoutNode {
    Leaf {
        pane_id: PaneId,
    },
    Split {
        direction: Direction,
        ratio: f32,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

impl LayoutNode {
    /// Count the number of leaf (pane) nodes in the tree.
    #[must_use]
    pub fn pane_count(&self) -> usize {
        match self {
            Self::Leaf { .. } => 1,
            Self::Split { first, second, .. } => first.pane_count() + second.pane_count(),
        }
    }

    /// Check whether the tree contains a pane with the given id.
    #[must_use]
    pub fn contains_pane(&self, pane_id: PaneId) -> bool {
        match self {
            Self::Leaf { pane_id: id } => *id == pane_id,
            Self::Split { first, second, .. } => {
                first.contains_pane(pane_id) || second.contains_pane(pane_id)
            }
        }
    }

    /// Collect all leaf pane IDs via in-order traversal.
    #[must_use]
    pub fn all_pane_ids(&self) -> Vec<PaneId> {
        let mut ids = Vec::new();
        self.collect_pane_ids(&mut ids);
        ids
    }

    fn collect_pane_ids(&self, ids: &mut Vec<PaneId>) {
        match self {
            Self::Leaf { pane_id } => ids.push(*pane_id),
            Self::Split { first, second, .. } => {
                first.collect_pane_ids(ids);
                second.collect_pane_ids(ids);
            }
        }
    }

    /// Recursively compute pixel rects for all panes given a viewport.
    #[must_use]
    pub fn compute_rects(&self, viewport: Rect) -> Vec<(PaneId, Rect)> {
        let mut rects = Vec::new();
        self.compute_rects_inner(viewport, &mut rects);
        rects
    }

    fn compute_rects_inner(&self, viewport: Rect, rects: &mut Vec<(PaneId, Rect)>) {
        match self {
            Self::Leaf { pane_id } => {
                rects.push((*pane_id, viewport));
            }
            Self::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let (first_rect, second_rect) = match direction {
                    Direction::Vertical => {
                        let first_width = viewport.width * ratio;
                        let second_width = viewport.width - first_width;
                        (
                            Rect::new(viewport.x, viewport.y, first_width, viewport.height),
                            Rect::new(
                                viewport.x + first_width,
                                viewport.y,
                                second_width,
                                viewport.height,
                            ),
                        )
                    }
                    Direction::Horizontal => {
                        let first_height = viewport.height * ratio;
                        let second_height = viewport.height - first_height;
                        (
                            Rect::new(viewport.x, viewport.y, viewport.width, first_height),
                            Rect::new(
                                viewport.x,
                                viewport.y + first_height,
                                viewport.width,
                                second_height,
                            ),
                        )
                    }
                };
                first.compute_rects_inner(first_rect, rects);
                second.compute_rects_inner(second_rect, rects);
            }
        }
    }

    /// Find the rect for a specific pane.
    #[must_use]
    pub fn find_pane_rect(&self, pane_id: PaneId, viewport: Rect) -> Option<Rect> {
        self.compute_rects(viewport)
            .into_iter()
            .find(|(id, _)| *id == pane_id)
            .map(|(_, rect)| rect)
    }

    /// Split a leaf pane into two panes. The original pane becomes the first
    /// child and the new pane becomes the second child. Returns `true` if the
    /// target pane was found and split.
    pub fn split_pane(
        &mut self,
        target_pane_id: PaneId,
        new_pane_id: PaneId,
        direction: Direction,
        ratio: f32,
    ) -> bool {
        match self {
            Self::Leaf { pane_id } => {
                if *pane_id == target_pane_id {
                    let old_id = *pane_id;
                    *self = Self::Split {
                        direction,
                        ratio,
                        first: Box::new(Self::Leaf { pane_id: old_id }),
                        second: Box::new(Self::Leaf {
                            pane_id: new_pane_id,
                        }),
                    };
                    true
                } else {
                    false
                }
            }
            Self::Split { first, second, .. } => {
                first.split_pane(target_pane_id, new_pane_id, direction, ratio)
                    || second.split_pane(target_pane_id, new_pane_id, direction, ratio)
            }
        }
    }

    /// Remove a pane from the tree, promoting its sibling in its place.
    /// Returns `true` if the pane was found and removed. Cannot remove the
    /// last pane.
    pub fn remove_pane(&mut self, target_pane_id: PaneId) -> bool {
        // Cannot remove the last pane.
        if self.pane_count() <= 1 {
            return false;
        }
        self.remove_pane_inner(target_pane_id)
    }

    fn remove_pane_inner(&mut self, target_pane_id: PaneId) -> bool {
        match self {
            Self::Leaf { .. } => false,
            Self::Split { first, second, .. } => {
                // Check if first child is the target leaf.
                if let Self::Leaf { pane_id } = first.as_ref()
                    && *pane_id == target_pane_id
                {
                    // Promote second child.
                    *self = std::mem::replace(
                        second.as_mut(),
                        Self::Leaf { pane_id: 0 }, // placeholder, immediately overwritten
                    );
                    return true;
                }
                // Check if second child is the target leaf.
                if let Self::Leaf { pane_id } = second.as_ref()
                    && *pane_id == target_pane_id
                {
                    // Promote first child.
                    *self = std::mem::replace(
                        first.as_mut(),
                        Self::Leaf { pane_id: 0 }, // placeholder, immediately overwritten
                    );
                    return true;
                }
                // Recurse into children.
                first.remove_pane_inner(target_pane_id) || second.remove_pane_inner(target_pane_id)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> Rect {
        Rect::new(0.0, 0.0, 800.0, 600.0)
    }

    #[test]
    fn single_pane_compute_rects() {
        let node = LayoutNode::Leaf { pane_id: 0 };
        let rects = node.compute_rects(viewport());
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].0, 0);
        assert_eq!(rects[0].1, viewport());
    }

    #[test]
    fn split_pane_creates_correct_tree() {
        let mut node = LayoutNode::Leaf { pane_id: 0 };
        assert!(node.split_pane(0, 1, Direction::Vertical, 0.5));
        assert_eq!(node.pane_count(), 2);
        assert!(node.contains_pane(0));
        assert!(node.contains_pane(1));
    }

    #[test]
    fn vertical_split_rects() {
        let mut node = LayoutNode::Leaf { pane_id: 0 };
        node.split_pane(0, 1, Direction::Vertical, 0.5);
        let rects = node.compute_rects(viewport());
        assert_eq!(rects.len(), 2);
        // First pane: left half.
        assert_eq!(rects[0].0, 0);
        assert!((rects[0].1.width - 400.0).abs() < f32::EPSILON);
        assert!((rects[0].1.x).abs() < f32::EPSILON);
        // Second pane: right half.
        assert_eq!(rects[1].0, 1);
        assert!((rects[1].1.width - 400.0).abs() < f32::EPSILON);
        assert!((rects[1].1.x - 400.0).abs() < f32::EPSILON);
    }

    #[test]
    fn horizontal_split_rects() {
        let mut node = LayoutNode::Leaf { pane_id: 0 };
        node.split_pane(0, 1, Direction::Horizontal, 0.5);
        let rects = node.compute_rects(viewport());
        assert_eq!(rects.len(), 2);
        // First pane: top half.
        assert_eq!(rects[0].0, 0);
        assert!((rects[0].1.height - 300.0).abs() < f32::EPSILON);
        // Second pane: bottom half.
        assert_eq!(rects[1].0, 1);
        assert!((rects[1].1.height - 300.0).abs() < f32::EPSILON);
        assert!((rects[1].1.y - 300.0).abs() < f32::EPSILON);
    }

    #[test]
    fn nested_splits_compute_correctly() {
        let mut node = LayoutNode::Leaf { pane_id: 0 };
        node.split_pane(0, 1, Direction::Vertical, 0.5);
        node.split_pane(1, 2, Direction::Horizontal, 0.5);
        let rects = node.compute_rects(viewport());
        assert_eq!(rects.len(), 3);
        // Pane 0: left half (400x600).
        assert_eq!(rects[0].0, 0);
        assert!((rects[0].1.width - 400.0).abs() < f32::EPSILON);
        // Pane 1: top-right (400x300).
        assert_eq!(rects[1].0, 1);
        assert!((rects[1].1.width - 400.0).abs() < f32::EPSILON);
        assert!((rects[1].1.height - 300.0).abs() < f32::EPSILON);
        // Pane 2: bottom-right (400x300).
        assert_eq!(rects[2].0, 2);
        assert!((rects[2].1.x - 400.0).abs() < f32::EPSILON);
        assert!((rects[2].1.y - 300.0).abs() < f32::EPSILON);
    }

    #[test]
    fn remove_pane_promotes_sibling() {
        let mut node = LayoutNode::Leaf { pane_id: 0 };
        node.split_pane(0, 1, Direction::Vertical, 0.5);
        assert!(node.remove_pane(0));
        assert_eq!(node.pane_count(), 1);
        assert!(node.contains_pane(1));
        assert!(!node.contains_pane(0));
    }

    #[test]
    fn remove_last_pane_returns_false() {
        let mut node = LayoutNode::Leaf { pane_id: 0 };
        assert!(!node.remove_pane(0));
    }

    #[test]
    fn all_pane_ids_order() {
        let mut node = LayoutNode::Leaf { pane_id: 0 };
        node.split_pane(0, 1, Direction::Vertical, 0.5);
        node.split_pane(1, 2, Direction::Horizontal, 0.5);
        assert_eq!(node.all_pane_ids(), vec![0, 1, 2]);
    }
}
