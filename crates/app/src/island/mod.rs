//! App-wide activity cache shared by every island window.
mod model;

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
