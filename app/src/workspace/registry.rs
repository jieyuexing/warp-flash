use std::collections::HashMap;

use warpui::{AppContext, Entity, SingletonEntity, WeakViewHandle, WindowId};

use super::Workspace;
use crate::app_state::WindowSnapshot;

/// A registry that tracks all workspace views by their window ID.
///
/// This provides O(1) lookup of workspaces instead of the O(n) linear scan
/// that `views_of_type::<Workspace>` performs.
pub struct WorkspaceRegistry {
    workspaces: HashMap<WindowId, WeakViewHandle<Workspace>>,
}

impl Default for WorkspaceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceRegistry {
    pub fn new() -> Self {
        Self {
            workspaces: HashMap::new(),
        }
    }

    /// Registers a workspace for the given window.
    pub fn register(&mut self, window_id: WindowId, workspace: WeakViewHandle<Workspace>) {
        self.workspaces.insert(window_id, workspace);
    }

    /// Unregisters the workspace for the given window.
    pub fn unregister(&mut self, window_id: WindowId) {
        self.workspaces.remove(&window_id);
    }

    /// Returns the workspace for the given window, if it is still alive.
    pub fn get(
        &self,
        window_id: WindowId,
        app: &AppContext,
    ) -> Option<warpui::ViewHandle<Workspace>> {
        self.workspaces.get(&window_id)?.upgrade(app)
    }

    /// Returns all registered workspaces that are still alive.
    /// The returned vector contains tuples of (WindowId, ViewHandle<Workspace>).
    pub fn all_workspaces(
        &self,
        app: &AppContext,
    ) -> Vec<(WindowId, warpui::ViewHandle<Workspace>)> {
        self.workspaces
            .iter()
            .filter_map(|(window_id, weak_handle)| {
                weak_handle.upgrade(app).map(|handle| (*window_id, handle))
            })
            .collect()
    }
}

impl Entity for WorkspaceRegistry {
    type Event = ();
}

impl SingletonEntity for WorkspaceRegistry {}

#[derive(Default)]
pub struct ClosedWorkspaceSnapshots {
    snapshots: Vec<(WindowId, WindowSnapshot)>,
}

impl ClosedWorkspaceSnapshots {
    pub fn insert(&mut self, window_id: WindowId, snapshot: WindowSnapshot) {
        if let Some((_, stored)) = self
            .snapshots
            .iter_mut()
            .find(|(stored_window_id, _)| *stored_window_id == window_id)
        {
            *stored = snapshot;
        } else {
            self.snapshots.push((window_id, snapshot));
        }
    }

    pub fn remove(&mut self, window_id: WindowId) {
        self.snapshots
            .retain(|(stored_window_id, _)| *stored_window_id != window_id);
    }

    pub fn snapshots(&self) -> impl Iterator<Item = &WindowSnapshot> {
        self.snapshots.iter().map(|(_, snapshot)| snapshot)
    }
}

impl Entity for ClosedWorkspaceSnapshots {
    type Event = ();
}

impl SingletonEntity for ClosedWorkspaceSnapshots {}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
