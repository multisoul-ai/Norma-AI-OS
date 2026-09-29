use crate::app::MultisoulApp;
use gpui::{AppContext as _, Context, Window};
use std::path::PathBuf;

impl MultisoulApp {
    pub(super) fn expand_desktop_project_area(
        &mut self,
        area_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let Some(area) = self
            .desktop_workspace
            .areas
            .iter()
            .find(|area| area.id == area_id)
            .cloned()
        else {
            return;
        };
        if let Some(error) = self.desktop_workspace.area_errors.get(&area_id) {
            self.desktop_workspace.error =
                Some(format!("Could not open {}: {error}", area.display_name));
            cx.notify();
            return;
        }
        let key = (workspace_id.clone(), area_id.clone());
        if let Some(session) = self.desktop_workspace.sessions.get(&key) {
            session.update(cx, |session, cx| {
                session.attach_desktop_area_window(window, cx)
            });
            window.focus(&session.read(cx).focus_handle);
            self.persist_desktop_view(workspace_id, Some(area_id.clone()), cx);
            self.desktop_workspace.expanded_area_id = Some(area_id);
            self.desktop_workspace.error = None;
            cx.notify();
            return;
        }
        self.ensure_desktop_area_session(area_id, true, window, cx);
    }

    pub(super) fn ensure_desktop_area_session(
        &mut self,
        area_id: String,
        expand: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let Some(area) = self
            .desktop_workspace
            .areas
            .iter()
            .find(|area| area.id == area_id)
            .cloned()
        else {
            return;
        };
        let key = (workspace_id.clone(), area_id.clone());
        if self.desktop_workspace.sessions.contains_key(&key)
            || self.desktop_workspace.area_errors.contains_key(&area_id)
            || self.desktop_workspace.loading_area_ids.contains(&key)
        {
            return;
        }
        self.desktop_workspace.loading_area_ids.insert(key.clone());
        cx.notify();
        let paths = self.database_paths.clone();
        let work = cx.background_executor().spawn(async move {
            crate::app::background_project::PreparedProject::load_for_desktop(paths, area)
        });
        cx.spawn_in(window, async move |this, cx| {
            let prepared = work.await;
            let _ = this.update_in(cx, |app, window, cx| {
                app.desktop_workspace.loading_area_ids.remove(&key);
                if app.desktop_workspace.discarded_area_loads.contains(&key) {
                    cx.notify();
                    return;
                }
                match prepared {
                    Ok(prepared) => {
                        let host = cx.weak_entity();
                        let session = cx.new(|cx| {
                            let mut session = MultisoulApp::from_background_project(prepared, cx);
                            session.desktop_host = Some(host);
                            session
                        });
                        if expand
                            && app.desktop_workspace.active_id.as_deref() == Some(&workspace_id)
                        {
                            session.update(cx, |session, cx| {
                                session.attach_desktop_area_window(window, cx)
                            });
                            window.focus(&session.read(cx).focus_handle);
                            app.persist_desktop_view(
                                workspace_id.clone(),
                                Some(area_id.clone()),
                                cx,
                            );
                            app.desktop_workspace.expanded_area_id = Some(area_id.clone());
                        } else {
                            session.update(cx, |session, cx| {
                                session.activate_desktop_area_overview(cx)
                            });
                        }
                        app.desktop_workspace.sessions.insert(key, session);
                    }
                    Err(error) => {
                        app.desktop_workspace
                            .area_errors
                            .insert(area_id, error.clone());
                        app.desktop_workspace.error =
                            Some(format!("Could not open project canvas: {error}"));
                    }
                }
                cx.notify();
            });
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }

    pub(super) fn collapse_desktop_project_area(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let (Some(workspace_id), Some(area_id)) = (
            self.desktop_workspace.active_id.as_deref(),
            self.desktop_workspace.expanded_area_id.as_deref(),
        ) {
            if let Some(session) = self
                .desktop_workspace
                .sessions
                .get(&(workspace_id.to_owned(), area_id.to_owned()))
            {
                session.update(cx, |session, cx| session.detach_project_window(cx));
            }
        }
        self.desktop_workspace.expanded_area_id = None;
        if let Some(workspace_id) = self.desktop_workspace.active_id.clone() {
            self.persist_desktop_view(workspace_id, None, cx);
        }
        window.focus(&self.focus_handle);
        cx.notify();
    }

    pub(crate) fn add_project_to_desktop_workspace(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        if self.desktop_workspace.busy {
            return;
        }
        self.desktop_workspace.busy = true;
        self.desktop_workspace.error = None;
        cx.notify();
        let store = self.desktop_workspace.store.clone();
        let store_id = id.clone();
        let work = cx.background_executor().spawn(async move {
            let area = store.add_project(&store_id, &path)?;
            let areas = store.list_projects(&store_id)?;
            let workspaces = store.list()?;
            Ok::<_, storage::StorageError>((area, areas, workspaces))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |app, window, cx| {
                app.desktop_workspace.busy = false;
                if app.desktop_workspace.active_id.as_deref() != Some(&id) {
                    cx.notify();
                    return;
                }
                match result {
                    Ok((area, areas, workspaces)) => {
                        let existing = app
                            .desktop_workspace
                            .areas
                            .iter()
                            .any(|item| item.id == area.id);
                        app.desktop_workspace.active_area_id = Some(area.id.clone());
                        app.desktop_workspace.areas = areas;
                        app.desktop_workspace.workspaces = workspaces;
                        if existing {
                            app.expand_desktop_project_area(area.id, window, cx);
                        }
                    }
                    Err(error) => {
                        app.desktop_workspace.error =
                            Some(format!("Could not add project: {error}"))
                    }
                }
                cx.notify();
            });
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }
}
