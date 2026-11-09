//! App-wide activity cache shared by every island window.
mod actions;
mod activity_view;
mod artifacts;
mod bloub_view;
mod body;
mod chrome;
mod controller;
mod faces;
mod fallback;
mod focus;
pub(crate) mod hotkey;
mod model;
#[cfg(target_os = "macos")]
mod native;
mod navigation;
#[cfg(target_os = "macos")]
mod notify;
mod pending;
pub(crate) mod preferences;
mod question;
mod question_editor;
mod queue_view;
mod remembered;
mod roster_view;
mod running;
mod running_filter;
pub(crate) mod staff;
mod staff_view;
mod style;
mod view;
mod windows;
pub(crate) use actions::register_source;
pub(crate) use preferences::install;
pub(crate) use windows::has_workspace_windows;

use gpui::{App, AppContext, Entity, Global};
pub(crate) use model::{IslandModel, IslandProject, IslandWorkspace};

struct IslandGlobal(Entity<IslandModel>);
impl Global for IslandGlobal {}

pub(crate) fn model(cx: &mut App) -> Entity<IslandModel> {
    if let Some(global) = cx.try_global::<IslandGlobal>() {
        return global.0.clone();
    }
    let model = cx.new(|_| IslandModel::default());
    cx.set_global(IslandGlobal(model.clone()));
    model
}

pub(crate) fn begin_refresh(cx: &mut App) -> u64 {
    model(cx).update(cx, |model, _| model.begin_refresh())
}

pub(crate) fn publish(
    source: u64,
    version: u64,
    database: std::path::PathBuf,
    workspaces: Vec<IslandWorkspace>,
    cx: &mut App,
) {
    model(cx).update(cx, |model, cx| {
        if model.replace_source(source, version, database, workspaces) {
            cx.notify();
        }
    });
}

pub(crate) fn remove_source(source: u64, cx: &mut App) {
    actions::unregister_source(source, cx);
    let Some(model) = cx
        .try_global::<IslandGlobal>()
        .map(|global| global.0.clone())
    else {
        return;
    };
    model.update(cx, |model, cx| {
        if model.remove_source(source) {
            cx.notify();
        }
    });
}

/// Host source plus the workspace, project and card that just finished.
/// The session entity id is not a locate source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CompletionRoute {
    pub source: u64,
    pub workspace: String,
    pub area: String,
    pub node: u64,
}

#[derive(Clone)]
pub(super) struct CompletionNotice {
    pub id: String,
    pub label: String,
    pub route: Option<CompletionRoute>,
}
impl gpui::EventEmitter<CompletionNotice> for IslandModel {}

pub(crate) fn task_completed(route: Option<CompletionRoute>, label: String, cx: &mut App) {
    let Some(route) = route else {
        return;
    };
    let id = faces::agent_id(route.source, &route.area, route.node);
    model(cx).update(cx, |_, cx| {
        cx.emit(CompletionNotice {
            id,
            label,
            route: Some(route),
        })
    });
}
