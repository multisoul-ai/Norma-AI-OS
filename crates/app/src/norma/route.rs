use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkspaceRouteError {
    WorkspaceRequired,
    WorkspaceNotFound,
    WorkspaceNotOpen,
    RouteExpired,
    AlreadyOpen,
}

struct ActiveWorkspaceRoute<T> {
    route_id: String,
    target: T,
}

pub(crate) struct WorkspaceRouteIndex<T> {
    active: HashMap<String, ActiveWorkspaceRoute<T>>,
    known: HashSet<String>,
}

impl<T> Default for WorkspaceRouteIndex<T> {
    fn default() -> Self {
        Self {
            active: HashMap::new(),
            known: HashSet::new(),
        }
    }
}

impl<T> WorkspaceRouteIndex<T> {
    pub(crate) fn register(
        &mut self,
        workspace_id: &str,
        route_id: &str,
        target: T,
    ) -> Result<(), WorkspaceRouteError> {
        if self.active.contains_key(workspace_id) {
            return Err(WorkspaceRouteError::AlreadyOpen);
        }

        self.known.insert(workspace_id.to_owned());
        self.active.insert(
            workspace_id.to_owned(),
            ActiveWorkspaceRoute {
                route_id: route_id.to_owned(),
                target,
            },
        );
        Ok(())
    }

    pub(crate) fn unregister(&mut self, workspace_id: &str, route_id: &str) -> bool {
        let owns_active_route = self
            .active
            .get(workspace_id)
            .is_some_and(|entry| entry.route_id == route_id);
        if owns_active_route {
            self.active.remove(workspace_id);
        }
        owns_active_route
    }

    pub(crate) fn resolve(
        &self,
        workspace_id: &str,
        route_id: &str,
    ) -> Result<&T, WorkspaceRouteError> {
        match self.active.get(workspace_id) {
            Some(entry) if entry.route_id == route_id => Ok(&entry.target),
            Some(_) => Err(WorkspaceRouteError::RouteExpired),
            None if self.known.contains(workspace_id) => Err(WorkspaceRouteError::WorkspaceNotOpen),
            None => Err(WorkspaceRouteError::WorkspaceNotFound),
        }
    }

    pub(crate) fn resolve_active(&self, workspace_id: &str) -> Result<&T, WorkspaceRouteError> {
        match self.active.get(workspace_id) {
            Some(entry) => Ok(&entry.target),
            None if self.known.contains(workspace_id) => Err(WorkspaceRouteError::WorkspaceNotOpen),
            None => Err(WorkspaceRouteError::WorkspaceNotFound),
        }
    }

    pub(crate) fn sole_active_target(&self) -> Option<&T> {
        if self.active.len() != 1 {
            return None;
        }
        self.active.values().next().map(|entry| &entry.target)
    }

    pub(crate) fn active_targets(&self) -> impl Iterator<Item = &T> {
        self.active.values().map(|entry| &entry.target)
    }
}
