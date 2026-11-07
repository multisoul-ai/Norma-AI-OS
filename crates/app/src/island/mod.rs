//! App-wide activity cache shared by every island window.
mod actions;
mod artifacts;
mod chrome;
mod controller;
mod model;
#[cfg(target_os = "macos")]
mod native;
mod navigation;
mod pending;
mod question;
mod question_editor;
mod remembered;
mod running;
mod style;
mod view;
mod windows;
pub(crate) use actions::register_source;
pub(crate) use windows::{has_workspace_windows, install};

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

#[derive(Clone)]
pub(super) struct CompletionNotice {
    pub id: String,
    pub label: String,
}
impl gpui::EventEmitter<CompletionNotice> for IslandModel {}

pub(crate) fn task_completed(source: u64, node: u64, label: String, cx: &mut App) {
    model(cx).update(cx, |_, cx| {
        cx.emit(CompletionNotice {
            id: format!("{source}:{node}"),
            label,
        })
    });
}
