//! 项目实体归进程持有，窗口只是可解除的显示引用；不创建隐藏窗口或第二个 Runtime。
use crate::app::MultisoulApp;
use gpui::{App, AppContext, Entity, Global, Task};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

type Key = (PathBuf, String);
#[derive(Default)]
struct Sessions {
    databases: HashMap<PathBuf, storage::DatabasePaths>,
    projects: HashMap<Key, Entity<MultisoulApp>>,
    loading: HashMap<Key, (u64, Vec<async_channel::Sender<()>>)>,
    next_load: u64,
    stopping: bool,
}
impl Global for Sessions {}

pub(crate) fn remember_database(paths: storage::DatabasePaths, cx: &mut App) {
    cx.default_global::<Sessions>()
        .databases
        .insert(paths.app_db_path().to_owned(), paths);
}

pub(crate) fn register(
    database: PathBuf,
    workspace: String,
    app: Entity<MultisoulApp>,
    cx: &mut App,
) {
    let registry = cx.default_global::<Sessions>();
    if registry.stopping {
        return;
    }
    registry
        .projects
        .retain(|key, value| key.0 != database || value.entity_id() != app.entity_id());
    registry.projects.insert((database, workspace), app);
}

pub(crate) fn remove(entity: gpui::EntityId, cx: &mut App) {
    cx.default_global::<Sessions>()
        .projects
        .retain(|_, app| app.entity_id() != entity);
}

pub(crate) fn cancel_loads(database: &Path, cx: &mut App) {
    cx.default_global::<Sessions>()
        .loading
        .retain(|key, _| key.0 != database);
}

pub(crate) fn shutdown(cx: &mut App) {
    let registry = cx.default_global::<Sessions>();
    registry.stopping = true;
    registry.loading.clear();
    let projects = std::mem::take(&mut registry.projects);
    crate::mobile::controller::shutdown(cx);
    for app in projects.into_values() {
        app.update(cx, |app, cx| app.stop_project(cx));
    }
}

pub(crate) fn can_quit(cx: &mut App) -> bool {
    if !super::desktop::sessions::can_quit(cx) {
        return false;
    }
    let projects = cx
        .default_global::<Sessions>()
        .projects
        .values()
        .cloned()
        .collect::<Vec<_>>();
    for app in projects {
        if !app.update(cx, |app, cx| app.confirm_preview_edits_before_leaving(cx)) {
            if let Some(window) = app.read(cx).project_window {
                let _ = window.update(cx, |_, window, _| window.activate_window());
            }
            return false;
        }
    }
    true
}

pub(crate) fn find(database: &Path, workspace: &str, cx: &App) -> Option<Entity<MultisoulApp>> {
    cx.try_global::<Sessions>()?
        .projects
        .get(&(database.to_owned(), workspace.to_owned()))
        .cloned()
}

pub(crate) fn for_database(database: &Path, cx: &App) -> Vec<Entity<MultisoulApp>> {
    cx.try_global::<Sessions>()
        .map(|sessions| {
            sessions
                .projects
                .iter()
                .filter(|((db, _), _)| db == database)
                .map(|(_, app)| app.clone())
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn for_root(database: &Path, root: &Path, cx: &App) -> Option<Entity<MultisoulApp>> {
    cx.try_global::<Sessions>()?
        .projects
        .iter()
        .find_map(|((db, _), app)| {
            (db == database && app.read(cx).runtime.workspace_root() == Some(root))
                .then(|| app.clone())
        })
}

/// Already-authorized metadata requests only. Concurrent readers share one background load.
pub(crate) fn ensure(
    database: &Path,
    workspace: &str,
    deadline: std::time::Instant,
    cx: &mut App,
) -> Task<()> {
    if find(database, workspace, cx).is_some() {
        return Task::ready(());
    }
    let key = (database.to_owned(), workspace.to_owned());
    let registry = cx.default_global::<Sessions>();
    if registry.stopping {
        return Task::ready(());
    }
    let Some(paths) = registry.databases.get(database).cloned() else {
        return Task::ready(());
    };
    let (sender, receiver) = async_channel::bounded(1);
    if let Some((_, waiters)) = registry.loading.get_mut(&key) {
        waiters.push(sender);
    } else {
        registry.next_load += 1;
        let generation = registry.next_load;
        registry
            .loading
            .insert(key.clone(), (generation, vec![sender]));
        let load_key = key.clone();
        let work = cx.background_executor().spawn(async move {
            crate::app::background_project::PreparedProject::load(paths, &load_key.1)
        });
        cx.spawn(async move |cx| {
            let prepared = work.await;
            let _ = cx.update(|cx| {
                if cx.global::<Sessions>().loading.get(&key).is_none_or(|(current, _)| *current != generation) { return; }
                if std::time::Instant::now() < deadline && find(&key.0, &key.1, cx).is_none() {
                    match prepared {
                        Ok(prepared) => {
                            let app = cx.new(|cx| MultisoulApp::from_background_project(prepared, cx));
                            app.update(cx, |app, cx| app.register_loaded_project_route(None, cx));
                        }
                        Err(error) => tracing::warn!(%error, workspace_id=%key.1, "background project load failed"),
                    }
                }
                if let Some((_, waiters)) = cx.default_global::<Sessions>().loading.remove(&key) {
                    for waiter in waiters { let _ = waiter.try_send(()); }
                }
            });
        }).detach();
    }
    cx.spawn(async move |_| {
        let _ = receiver.recv().await;
    })
}

/// Window-close handling also runs when other project windows remain open.
pub(crate) fn windows_closed(cx: &mut App) -> bool {
    let live = cx.windows();
    let projects = cx
        .default_global::<Sessions>()
        .projects
        .iter()
        .map(|(key, app)| (key.clone(), app.clone()))
        .collect::<Vec<_>>();
    for (key, app) in projects {
        let (closed, retain) = app.read_with(cx, |app, cx| {
            (
                app.project_window
                    .is_some_and(|window| !live.contains(&window)),
                app.background_project || app.mobile.read(cx).enabled(),
            )
        });
        if !closed {
            continue;
        }
        if retain {
            app.update(cx, |app, cx| app.detach_project_window(cx));
        } else {
            cx.default_global::<Sessions>().projects.remove(&key);
            app.update(cx, |app, cx| app.stop_project(cx));
        }
    }
    crate::mobile::controller::has_enabled_service(cx)
        || cx
            .global::<Sessions>()
            .projects
            .values()
            .any(|app| app.read(cx).background_project)
}
