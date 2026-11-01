//! Shared appearance routing for desktop hosts and independent project windows.
use super::MultisoulApp;
use gpui::{App, Entity};
use std::{collections::HashSet, path::Path};

pub(super) fn coordinator(app: &MultisoulApp, cx: &App) -> Option<Entity<MultisoulApp>> {
    crate::workspace::desktop::sessions::hosts(app.database_paths.app_db_path(), cx)
        .into_iter()
        .next()
        .or_else(|| app.desktop_host.as_ref().and_then(|host| host.upgrade()))
}

/// Call after deferring to the next UI tick: a project can initiate a change while its host renders.
pub(super) fn views(
    database: &Path,
    sender: Entity<MultisoulApp>,
    cx: &App,
) -> Vec<Entity<MultisoulApp>> {
    let mut views = crate::workspace::desktop::sessions::hosts(database, cx);
    views.extend(crate::workspace::sessions::for_database(database, cx));
    views.push(sender);
    let children: Vec<_> = views
        .iter()
        .flat_map(|view| {
            view.read(cx)
                .desktop_workspace
                .sessions
                .values()
                .cloned()
                .collect::<Vec<_>>()
        })
        .collect();
    views.extend(children);
    let mut seen = HashSet::new();
    views.retain(|view| seen.insert(view.entity_id()));
    views
}
