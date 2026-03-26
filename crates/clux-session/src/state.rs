use std::path::PathBuf;

use clux_layout::pane::PaneId;
use clux_layout::tab::Tab;
use clux_layout::tree::{Direction, LayoutNode};
use serde::{Deserialize, Serialize};

/// Serializable snapshot of the entire workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub name: String,
    pub active_tab: usize,
    pub tabs: Vec<TabState>,
    pub panes: Vec<PaneSnapshot>,
}

/// Serializable snapshot of a single tab.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabState {
    pub name: String,
    pub active_pane: PaneId,
    pub layout: LayoutNodeState,
}

/// Mirror of `LayoutNode` that is always serializable.
/// We reuse the derives already on `LayoutNode` via `From` conversions,
/// but keep a dedicated type so `clux-session` owns its schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LayoutNodeState {
    Leaf {
        pane_id: PaneId,
    },
    Split {
        direction: Direction,
        ratio: f32,
        first: Box<LayoutNodeState>,
        second: Box<LayoutNodeState>,
    },
}

/// Metadata about a single pane needed for restoration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneSnapshot {
    pub pane_id: PaneId,
    pub cwd: Option<PathBuf>,
    pub shell: String,
    pub cols: u16,
    pub rows: u16,
}

// ---------------------------------------------------------------------------
// Conversions
// ---------------------------------------------------------------------------

impl From<&LayoutNode> for LayoutNodeState {
    fn from(node: &LayoutNode) -> Self {
        match node {
            LayoutNode::Leaf { pane_id } => Self::Leaf { pane_id: *pane_id },
            LayoutNode::Split {
                direction,
                ratio,
                first,
                second,
            } => Self::Split {
                direction: *direction,
                ratio: *ratio,
                first: Box::new(Self::from(first.as_ref())),
                second: Box::new(Self::from(second.as_ref())),
            },
        }
    }
}

impl From<&LayoutNodeState> for LayoutNode {
    fn from(state: &LayoutNodeState) -> Self {
        match state {
            LayoutNodeState::Leaf { pane_id } => Self::Leaf { pane_id: *pane_id },
            LayoutNodeState::Split {
                direction,
                ratio,
                first,
                second,
            } => Self::Split {
                direction: *direction,
                ratio: *ratio,
                first: Box::new(Self::from(first.as_ref())),
                second: Box::new(Self::from(second.as_ref())),
            },
        }
    }
}

impl From<&Tab> for TabState {
    fn from(tab: &Tab) -> Self {
        Self {
            name: tab.name.clone(),
            active_pane: tab.active_pane,
            layout: LayoutNodeState::from(tab.layout()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clux_layout::tree::Direction;

    #[test]
    fn layout_node_roundtrip() {
        let node = LayoutNode::Split {
            direction: Direction::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf { pane_id: 0 }),
            second: Box::new(LayoutNode::Leaf { pane_id: 1 }),
        };
        let state = LayoutNodeState::from(&node);
        let restored = LayoutNode::from(&state);

        // Verify via serialization that they match.
        let original_json = serde_json::to_string(&node).unwrap();
        let restored_json = serde_json::to_string(&restored).unwrap();
        assert_eq!(original_json, restored_json);
    }

    #[test]
    fn session_state_serializes() {
        let session = SessionState {
            name: "test".to_string(),
            active_tab: 0,
            tabs: vec![TabState {
                name: "main".to_string(),
                active_pane: 0,
                layout: LayoutNodeState::Leaf { pane_id: 0 },
            }],
            panes: vec![PaneSnapshot {
                pane_id: 0,
                cwd: Some(PathBuf::from("C:\\Users\\test")),
                shell: "pwsh.exe".to_string(),
                cols: 120,
                rows: 30,
            }],
        };
        let json = serde_json::to_string_pretty(&session).unwrap();
        let deserialized: SessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "test");
        assert_eq!(deserialized.tabs.len(), 1);
        assert_eq!(deserialized.panes.len(), 1);
    }
}
