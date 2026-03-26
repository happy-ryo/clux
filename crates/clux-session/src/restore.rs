use std::path::PathBuf;

use clux_layout::pane::PaneId;
use clux_layout::tree::LayoutNode;

use crate::state::{LayoutNodeState, SessionState};

/// Describes how to recreate one pane.
#[derive(Debug)]
pub struct PaneSpec {
    pub pane_id: PaneId,
    pub shell: String,
    pub cwd: Option<PathBuf>,
    pub cols: u16,
    pub rows: u16,
}

/// Describes how to recreate one tab.
#[derive(Debug)]
pub struct TabSpec {
    pub name: String,
    pub active_pane: PaneId,
    pub layout: LayoutNode,
}

/// A plan for restoring a full session without performing any I/O.
/// The caller (main app) uses this plan to spawn `ConPTY` processes and
/// build tabs/panes.
#[derive(Debug)]
pub struct RestorationPlan {
    pub session_name: String,
    pub active_tab: usize,
    pub tabs: Vec<TabSpec>,
    pub panes: Vec<PaneSpec>,
}

/// Convert a saved session state into a restoration plan.
///
/// The plan describes *what* to create but does not perform side effects.
/// The caller is responsible for spawning terminals and wiring up buffers.
#[must_use]
pub fn plan_restoration(state: &SessionState) -> RestorationPlan {
    let tabs = state
        .tabs
        .iter()
        .map(|tab_state| TabSpec {
            name: tab_state.name.clone(),
            active_pane: tab_state.active_pane,
            layout: layout_node_from_state(&tab_state.layout),
        })
        .collect();

    let panes = state
        .panes
        .iter()
        .map(|ps| PaneSpec {
            pane_id: ps.pane_id,
            shell: ps.shell.clone(),
            cwd: ps.cwd.clone(),
            cols: ps.cols,
            rows: ps.rows,
        })
        .collect();

    RestorationPlan {
        session_name: state.name.clone(),
        active_tab: state.active_tab,
        tabs,
        panes,
    }
}

fn layout_node_from_state(state: &LayoutNodeState) -> LayoutNode {
    LayoutNode::from(state)
}

/// Collect all pane IDs referenced in a layout tree.
#[must_use]
pub fn collect_pane_ids(layout: &LayoutNodeState) -> Vec<PaneId> {
    let mut ids = Vec::new();
    collect_pane_ids_inner(layout, &mut ids);
    ids
}

fn collect_pane_ids_inner(layout: &LayoutNodeState, ids: &mut Vec<PaneId>) {
    match layout {
        LayoutNodeState::Leaf { pane_id } => ids.push(*pane_id),
        LayoutNodeState::Split { first, second, .. } => {
            collect_pane_ids_inner(first, ids);
            collect_pane_ids_inner(second, ids);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{LayoutNodeState, PaneSnapshot, SessionState, TabState};
    use clux_layout::tree::Direction;
    use std::path::PathBuf;

    #[test]
    fn plan_restoration_produces_correct_plan() {
        let state = SessionState {
            name: "test".to_string(),
            active_tab: 0,
            tabs: vec![TabState {
                name: "main".to_string(),
                active_pane: 1,
                layout: LayoutNodeState::Split {
                    direction: Direction::Vertical,
                    ratio: 0.5,
                    first: Box::new(LayoutNodeState::Leaf { pane_id: 0 }),
                    second: Box::new(LayoutNodeState::Leaf { pane_id: 1 }),
                },
            }],
            panes: vec![
                PaneSnapshot {
                    pane_id: 0,
                    cwd: Some(PathBuf::from("C:\\project")),
                    shell: "pwsh.exe".to_string(),
                    cols: 80,
                    rows: 24,
                },
                PaneSnapshot {
                    pane_id: 1,
                    cwd: None,
                    shell: "pwsh.exe".to_string(),
                    cols: 80,
                    rows: 24,
                },
            ],
        };

        let plan = plan_restoration(&state);
        assert_eq!(plan.session_name, "test");
        assert_eq!(plan.active_tab, 0);
        assert_eq!(plan.tabs.len(), 1);
        assert_eq!(plan.panes.len(), 2);
        assert_eq!(plan.tabs[0].active_pane, 1);
    }

    #[test]
    fn collect_pane_ids_finds_all() {
        let layout = LayoutNodeState::Split {
            direction: Direction::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNodeState::Leaf { pane_id: 0 }),
            second: Box::new(LayoutNodeState::Split {
                direction: Direction::Horizontal,
                ratio: 0.5,
                first: Box::new(LayoutNodeState::Leaf { pane_id: 1 }),
                second: Box::new(LayoutNodeState::Leaf { pane_id: 2 }),
            }),
        };
        let ids = collect_pane_ids(&layout);
        assert_eq!(ids, vec![0, 1, 2]);
    }
}
