use crate::app::MultisoulApp;
use gpui::{AppContext as _, Context, Window};
use std::path::PathBuf;

impl MultisoulApp {
    pub(super) fn retry_desktop_project_area(
        &mut self,
        area_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let Some(root) = self
            .desktop_workspace
            .areas
            .iter()
            .find(|area| area.id == area_id)
            .map(|area| area.root.clone())
        else {
            return;
        };
        let work = cx.background_executor().spawn(async move {
            root.is_dir() && root.canonicalize().is_ok_and(|canonical| canonical == root)
        });
        cx.spawn_in(window, async move |this, cx| {
            let available = work.await;
            let _ = this.update_in(cx, |app, window, cx| {
                if app.desktop_workspace.active_id.as_deref() != Some(&workspace_id) {
                    return;
                }
                if available {
                    app.desktop_workspace.area_errors.remove(&area_id);
                    app.desktop_workspace.error = None;
                    app.ensure_desktop_area_session(area_id, window, cx);
                } else {
                    app.desktop_workspace.error =
                        Some("Project directory is still unavailable".to_string());
                }
                cx.notify();
            });
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }

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
        self.desktop_workspace.active_area_id = Some(area_id.clone());
        let key = (workspace_id.clone(), area_id.clone());
        if let Some(session) = self.desktop_workspace.sessions.get(&key).cloned() {
            let previous = self
                .desktop_workspace
                .transition
                .as_ref()
                .and_then(|transition| match &transition.subject {
                    super::camera::DesktopCanvasSubject::Project { area_id, .. } => {
                        Some(area_id.clone())
                    }
                    _ => None,
                })
                .or_else(|| self.desktop_workspace.expanded_area_id.clone());
            if let Some(previous) = previous.filter(|previous| previous != &area_id) {
                self.begin_desktop_project_switch(&workspace_id, &previous, &area_id, window, cx);
                if let Some(previous) = self
                    .desktop_workspace
                    .sessions
                    .get(&(workspace_id.clone(), previous))
                {
                    previous.update(cx, |app, cx| app.detach_project_window(cx));
                }
            } else if self.desktop_workspace.expanded_area_id.as_deref() == Some(&area_id) {
                self.desktop_workspace.pending_navigation = None;
            } else if matches!(self.screen, crate::app::AppScreen::Launchpad)
                && self.desktop_workspace.transition.is_none()
            {
                self.begin_desktop_project_directory_zoom(
                    &workspace_id,
                    &area_id,
                    false,
                    window,
                    cx,
                );
            } else {
                if self.defer_desktop_navigation(
                    super::scene::DesktopNavigation::Project(area_id.clone()),
                    window,
                    cx,
                ) {
                    return;
                }
                self.begin_desktop_project_zoom(&workspace_id, &area_id, false, window, cx);
            }
            let wallpaper = self.wallpaper_settings.selection.clone();
            session.update(cx, |session, cx| {
                session.wallpaper_settings.selection = wallpaper;
                session.attach_desktop_area_window(window, cx);
            });
            window.focus(&session.read(cx).focus_handle);
            self.persist_desktop_view(workspace_id, Some(area_id.clone()), cx);
            self.desktop_workspace.expanded_area_id = Some(area_id);
            self.screen = crate::app::AppScreen::DesktopWorkspace;
            self.desktop_workspace.error = None;
            cx.notify();
            return;
        }
        if self.desktop_workspace.expanded_area_id.is_some() {
            // Keep the current canvas visible until the requested project's session is ready.
            self.desktop_workspace.pending_navigation =
                Some(super::scene::DesktopNavigation::Project(area_id.clone()));
        } else {
            self.desktop_workspace.expanded_area_id = Some(area_id.clone());
        }
        self.ensure_desktop_area_session(area_id, window, cx);
    }

    pub(super) fn ensure_desktop_area_session(
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
                            session.layout_strategy.chrome_in_parent = true;
                            session.master_layout.chrome_in_parent = true;
                            session
                        });
                        // Keep window-level Settings live without redrawing the host for
                        // ordinary terminal output while the panel is closed.
                        let mut settings_were_open = false;
                        cx.observe(&session, move |_, session, cx| {
                            let open = session.read(cx).settings_panel.open;
                            if open || settings_were_open {
                                cx.notify();
                            }
                            settings_were_open = open;
                        })
                        .detach();
                        let requested = match &app.desktop_workspace.pending_navigation {
                            Some(super::scene::DesktopNavigation::Project(id)) => id == &area_id,
                            _ => {
                                app.desktop_workspace.expanded_area_id.as_deref() == Some(&area_id)
                            }
                        };
                        let visible = app.desktop_workspace.active_id.as_deref()
                            == Some(&workspace_id)
                            && matches!(app.screen, crate::app::AppScreen::DesktopWorkspace);
                        let expand_now =
                            requested && visible && !app.desktop_workspace_zoom_active();
                        if !expand_now {
                            session.update(cx, |session, cx| {
                                session.activate_desktop_area_overview(cx)
                            });
                        }
                        app.desktop_workspace.sessions.insert(key, session);
                        if expand_now {
                            app.expand_desktop_project_area(area_id, window, cx);
                        } else if requested && visible {
                            app.desktop_workspace.expanded_area_id = Some(area_id);
                        }
                    }
                    Err(error) => {
                        if matches!(&app.desktop_workspace.pending_navigation,
                            Some(super::scene::DesktopNavigation::Project(id)) if id == &area_id)
                        {
                            app.desktop_workspace.pending_navigation = None;
                        }
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
        self.focus_handle.focus(window);
        self.desktop_workspace.pending_navigation = None;
        if let (Some(workspace_id), Some(area_id)) = (
            self.desktop_workspace.active_id.clone(),
            self.desktop_workspace.expanded_area_id.clone().or_else(|| {
                self.desktop_workspace
                    .transition
                    .as_ref()
                    .and_then(|t| match &t.subject {
                        super::camera::DesktopCanvasSubject::Project { area_id, .. } => {
                            Some(area_id.clone())
                        }
                        _ => None,
                    })
            }),
        ) {
            if !self.desktop_workspace_zoom_active() {
                self.begin_desktop_project_zoom(&workspace_id, &area_id, true, window, cx);
            }
            if self.desktop_workspace.transition.is_none() {
                if let Some(session) = self
                    .desktop_workspace
                    .sessions
                    .get(&(workspace_id, area_id))
                {
                    session.update(cx, |session, cx| session.detach_project_window(cx));
                }
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
        self.add_project_to_desktop_workspace_with_source(
            path,
            workspace::WorkspaceSource::Open,
            window,
            cx,
        );
    }

    pub(crate) fn add_project_to_desktop_workspace_with_source(
        &mut self,
        path: PathBuf,
        source: workspace::WorkspaceSource,
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
        let recent_store = storage::RecentStore::new(self.database_paths.app_db_path().to_owned());
        let store_id = id.clone();
        let work = cx.background_executor().spawn(async move {
            let area = store.add_project(&store_id, &path)?;
            let areas = store.list_projects(&store_id)?;
            let workspaces = store.list()?;
            let recent = recent_store.record_opened(&path, source).ok();
            Ok::<_, storage::StorageError>((area, areas, workspaces, recent))
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
                    Ok((area, areas, workspaces, recent)) => {
                        if let Some(recent) = recent {
                            app.workspace_state.recent = recent;
                        }
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
                        } else {
                            app.ensure_desktop_area_session(area.id, window, cx);
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
