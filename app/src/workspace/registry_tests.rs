use warpui::WindowId;

use super::ClosedWorkspaceSnapshots;
use crate::app_state::WindowSnapshot;

fn snapshot(vertical_tabs_panel_open: bool) -> WindowSnapshot {
    WindowSnapshot {
        tabs: vec![],
        archived_tabs: vec![],
        active_tab_index: 0,
        team_uid: None,
        bounds: None,
        fullscreen_state: Default::default(),
        quake_mode: false,
        universal_search_width: None,
        warp_ai_width: None,
        voltron_width: None,
        warp_drive_index_width: None,
        left_panel_open: false,
        vertical_tabs_panel_open,
        vertical_tabs_panel_width: None,
        archived_tabs_expanded: false,
        left_panel_width: None,
        right_panel_width: None,
        agent_management_filters: None,
        tab_groups: vec![],
    }
}

#[test]
fn closed_workspace_snapshots_keep_insertion_order_when_replaced() {
    let first_window = WindowId::from_usize(1);
    let second_window = WindowId::from_usize(2);
    let mut snapshots = ClosedWorkspaceSnapshots::default();
    snapshots.insert(first_window, snapshot(false));
    snapshots.insert(second_window, snapshot(false));

    snapshots.insert(first_window, snapshot(true));

    let restored = snapshots.snapshots().collect::<Vec<_>>();
    assert_eq!(restored.len(), 2);
    assert!(restored[0].vertical_tabs_panel_open);
    assert!(!restored[1].vertical_tabs_panel_open);
}

#[test]
fn closed_workspace_snapshots_remove_only_matching_window() {
    let first_window = WindowId::from_usize(1);
    let second_window = WindowId::from_usize(2);
    let mut snapshots = ClosedWorkspaceSnapshots::default();
    snapshots.insert(first_window, snapshot(false));
    snapshots.insert(second_window, snapshot(true));

    snapshots.remove(first_window);

    let restored = snapshots.snapshots().collect::<Vec<_>>();
    assert_eq!(restored.len(), 1);
    assert!(restored[0].vertical_tabs_panel_open);
}
