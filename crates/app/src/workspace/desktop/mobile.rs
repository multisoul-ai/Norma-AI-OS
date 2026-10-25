//! Resolve exact retained desktop canvases without changing visible desktop selection.
use crate::app::MultisoulApp;
use gpui::{App, AppContext, Entity, Task};
use std::{
    path::Path,
    time::{Duration, Instant},
};
use storage::DesktopProjectArea;

pub(crate) fn session(
    database: &Path,
    area: &DesktopProjectArea,
    cx: &App,
) -> Option<Entity<MultisoulApp>> {
    let key = (area.desktop_workspace_id.clone(), area.id.clone());
    let matches: Vec<_> = super::sessions::hosts(database, cx)
        .into_iter()
        .filter_map(|host| host.read(cx).desktop_workspace.sessions.get(&key).cloned())
        .collect();
    (matches.len() == 1).then(|| matches[0].clone())
}

pub(crate) fn ensure(
    database: &Path,
    area: DesktopProjectArea,
    cx: &mut App,
) -> Task<Result<Entity<MultisoulApp>, String>> {
    let database = database.to_path_buf();
    cx.spawn(async move |cx| {
        let existing = cx
            .update(|cx| session(&database, &area, cx))
            .map_err(|e| e.to_string())?;
        if let Some(existing) = existing {
            return Ok(existing);
        }
        let hosts = cx
            .update(|cx| super::sessions::hosts(&database, cx))
            .map_err(|e| e.to_string())?;
        if hosts.len() != 1 {
            return Err("desktop_host_unavailable".into());
        }
        let host = hosts[0].clone();
        let key = (area.desktop_workspace_id.clone(), area.id.clone());
        let deadline = Instant::now() + Duration::from_secs(20);
        let claimed = host
            .update(cx, |app, _| {
                app.desktop_workspace.loading_area_ids.insert(key.clone())
            })
            .map_err(|e| e.to_string())?;
        if !claimed {
            loop {
                if let Some(existing) = cx
                    .update(|cx| session(&database, &area, cx))
                    .map_err(|e| e.to_string())?
                {
                    return Ok(existing);
                }
                if Instant::now() >= deadline {
                    return Err("project_load_timeout".into());
                }
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
            }
        }
        let paths = host
            .read_with(cx, |app, _| app.database_paths.clone())
            .map_err(|e| e.to_string())?;
        let load_area = area.clone();
        let prepared = cx
            .background_executor()
            .spawn(async move {
                crate::app::background_project::PreparedProject::load_for_desktop(paths, load_area)
            })
            .await;
        host.update(cx, |app, cx| {
            app.desktop_workspace.loading_area_ids.remove(&key);
            if app.desktop_workspace.discarded_area_loads.contains(&key) {
                return Err("configuration_changed".into());
            }
            if let Some(existing) = app.desktop_workspace.sessions.get(&key) {
                return Ok(existing.clone());
            }
            let prepared = prepared?;
            let owner = cx.weak_entity();
            let session = cx.new(|cx| {
                let mut session = MultisoulApp::from_background_project(prepared, cx);
                session.desktop_host = Some(owner);
                session.background_project = true;
                session.window_active = false;
                session.layout_strategy.chrome_in_parent = true;
                session.master_layout.bind_desktop(&session.runtime);
                session
            });
            session.update(cx, |session, cx| {
                session.register_loaded_project_route(None, cx)
            });
            app.desktop_workspace.sessions.insert(key, session.clone());
            app.sync_all_dispatch_effects(cx);
            app.mark_workspace_activity_dirty(cx);
            cx.notify();
            Ok(session)
        })
        .map_err(|e| e.to_string())?
    })
}
