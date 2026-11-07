//! Exact source/workspace routing for island actions; never uses the visible workspace.
use crate::app::MultisoulApp;
use gpui::{App, Entity, Global, WeakEntity};
use std::collections::BTreeMap;

#[derive(Default)]
struct Sources(BTreeMap<u64, WeakEntity<MultisoulApp>>);
impl Global for Sources {}

pub(crate) fn register_source(app: &Entity<MultisoulApp>, cx: &mut App) {
    cx.default_global::<Sources>()
        .0
        .insert(app.entity_id().as_u64(), app.downgrade());
}

/// Live apps that can carry a request to the brain: every project session of every host (the
/// workspace identity lives on the session) followed by the hosts themselves.
pub(super) fn hosts(cx: &App) -> Vec<Entity<MultisoulApp>> {
    let hosts: Vec<_> = cx
        .try_global::<Sources>()
        .map(|sources| sources.0.values().filter_map(WeakEntity::upgrade).collect())
        .unwrap_or_default();
    let mut apps: Vec<_> = hosts
        .iter()
        .flat_map(|host| host.read(cx).desktop_workspace.sessions.values().cloned())
        .collect();
    apps.extend(hosts);
    apps
}

pub(super) fn unregister_source(source: u64, cx: &mut App) {
    cx.default_global::<Sources>().0.remove(&source);
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct IslandTarget {
    pub source: u64,
    pub workspace: String,
    pub area: String,
}

impl IslandTarget {
    pub fn new(workspace: &super::IslandWorkspace, project: &super::IslandProject) -> Self {
        Self {
            source: project.source,
            workspace: workspace.id.clone(),
            area: project.area_id.clone(),
        }
    }

    pub fn host(&self, cx: &App) -> Option<Entity<MultisoulApp>> {
        cx.try_global::<Sources>()?.0.get(&self.source)?.upgrade()
    }

    pub fn session(&self, cx: &App) -> Option<Entity<MultisoulApp>> {
        self.host(cx)?
            .read(cx)
            .desktop_workspace
            .sessions
            .get(&(self.workspace.clone(), self.area.clone()))
            .cloned()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/island_actions_tests.rs"]
mod tests;
