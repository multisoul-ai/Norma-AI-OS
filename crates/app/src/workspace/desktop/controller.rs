use crate::app::{AppScreen, MultisoulApp};
use gpui::{Context, Window};
use std::path::PathBuf;
use std::time::Duration;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_workspace_creation_tests.rs"]
mod creation_tests;

impl MultisoulApp {
    pub(crate) fn refresh_desktop_previews(&mut self, cx: &mut Context<Self>) {
        let store = self.desktop_workspace.store.clone();
        let wallpaper_revision = self.wallpaper_settings.scoped.revision;
        let work = cx.background_executor().spawn(async move {
            Ok::<_, storage::StorageError>((store.list()?, store.list_previews()?))
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| match result {
                Ok((mut workspaces, previews)) => {
                    app.preserve_wallpaper_snapshot(
                        &mut workspaces,
                        &mut [],
                        wallpaper_revision,
                        cx,
                    );
                    app.desktop_workspace.workspaces = workspaces;
                    app.desktop_workspace.previews = previews;
                    app.bind_desktop_wallpaper(cx);
                    app.broadcast_wallpaper_state(None, cx);
                    cx.notify();
                }
                Err(error) => {
                    app.desktop_workspace.error = Some(ui::i18n::message(
                        "workspace.could_not_load_workspace_previews_error",
                        &[("error", (error).to_string())],
                    ));
                    cx.notify();
                }
            });
        })
        .detach();
    }
    pub(crate) fn accept_project_for_desktop(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.accept_project_for_desktop_with_source(
            path,
            workspace::WorkspaceSource::Open,
            window,
            cx,
        );
    }

    pub(crate) fn accept_project_for_desktop_with_source(
        &mut self,
        path: PathBuf,
        source: workspace::WorkspaceSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            host.update(cx, |host, cx| {
                host.accept_project_for_desktop_with_source(path, source, window, cx)
            });
            return;
        }
        if matches!(self.screen, AppScreen::DesktopWorkspace)
            && self.desktop_workspace.active_id.is_some()
        {
            self.add_project_to_desktop_workspace_with_source(path, source, window, cx);
        } else {
            self.desktop_workspace.pending_project_path = Some(path);
            self.desktop_workspace.pending_project_source = source;
            cx.notify();
        }
    }

    pub(super) fn cancel_desktop_project_destination(&mut self, cx: &mut Context<Self>) {
        if self.desktop_workspace.busy {
            return;
        }
        self.desktop_workspace.pending_project_path = None;
        self.desktop_workspace.pending_project_source = workspace::WorkspaceSource::Open;
        cx.notify();
    }

    pub(super) fn choose_desktop_project_destination(
        &mut self,
        target_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.desktop_workspace.busy {
            return;
        }
        let Some(id) = target_id else {
            self.create_desktop_workspace(window, cx);
            return;
        };
        let Some(path) = self.desktop_workspace.pending_project_path.clone() else {
            return;
        };
        let source = self.desktop_workspace.pending_project_source;
        self.desktop_workspace.busy = true;
        self.desktop_workspace.error = None;
        cx.notify();
        let store = self.desktop_workspace.store.clone();
        let recent_store = storage::RecentStore::new(self.database_paths.app_db_path().to_owned());
        let work = cx.background_executor().spawn(async move {
            store.add_project(&id, &path)?;
            let recent = recent_store.record_opened(&path, source).ok();
            Ok::<_, storage::StorageError>((id, store.list()?, recent))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |app, window, cx| {
                app.desktop_workspace.busy = false;
                match result {
                    Ok((id, workspaces, recent)) => {
                        app.desktop_workspace.pending_project_path = None;
                        app.desktop_workspace.pending_project_source =
                            workspace::WorkspaceSource::Open;
                        if let Some(recent) = recent {
                            app.workspace_state.recent = recent;
                        }
                        app.desktop_workspace.workspaces = workspaces;
                        app.open_desktop_workspace(id, window, cx);
                    }
                    Err(error) => {
                        app.desktop_workspace.error = Some(ui::i18n::message(
                            "workspace.could_not_add_project_error",
                            &[("error", (error).to_string())],
                        ))
                    }
                }
                cx.notify();
            });
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }
    pub(super) fn open_desktop_workspace(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.allow_workbench_workspace_switch(&id, window, cx) {
            return;
        }
        if !self
            .desktop_workspace
            .workspaces
            .iter()
            .any(|item| item.id == id)
        {
            return;
        }
        self.focus_handle.focus(window);
        if let Some(dock) = self.command_dock.as_mut() {
            dock.sidebar_origin = true;
        }
        if self.desktop_workspace.switcher_open {
            self.desktop_workspace.search_query = self.desktop_workspace.switcher_query.clone();
        }
        if self.desktop_workspace.active_id.as_deref() == Some(&id)
            && (self.desktop_project_zoom_active()
                || self.desktop_workspace.expanded_area_id.is_some())
        {
            self.screen = AppScreen::DesktopWorkspace;
            self.collapse_desktop_project_area(window, cx);
            self.close_desktop_switcher(cx);
            return;
        }
        if self.defer_desktop_navigation(
            super::scene::DesktopNavigation::Workspace(id.clone()),
            window,
            cx,
        ) {
            return;
        }
        self.detach_visible_desktop_area(cx);
        // 侧栏先按目标空间落定，相机终点才是最终布局。
        self.activate_desktop_workspace_view(&id, cx);
        self.begin_desktop_workspace_zoom(&id, false, window, cx);
        self.hide_desktop_content_browsers();
        self.start_desktop_status_refresh(cx);
        self.desktop_workspace.workspace_load_generation = self
            .desktop_workspace
            .workspace_load_generation
            .wrapping_add(1);
        self.reset_desktop_content_tabs();
        self.desktop_workspace.areas.clear();
        self.desktop_workspace.workbench = Default::default();
        self.desktop_workspace.content_artifacts.clear();
        self.desktop_workspace.status_cache.clear();
        self.desktop_workspace.status_live_seen.clear();
        self.invalidate_workspace_activity();
        self.desktop_workspace.artifact_revision = 0;
        self.desktop_workspace.active_area_id = None;
        self.desktop_workspace.selected_area_id = None;
        self.desktop_workspace.expanded_area_id = None;
        self.desktop_workspace.actions_area_id = None;
        self.close_desktop_switcher(cx);
        self.desktop_workspace.busy = true;
        self.screen = AppScreen::DesktopWorkspace;
        self.desktop_workspace.error = None;
        self.bind_desktop_wallpaper(cx);
        self.refresh_wallpaper_library(cx);
        cx.notify();
        let store = self.desktop_workspace.store.clone();
        let content_tabs =
            storage::DesktopContentTabStore::new(self.database_paths.app_db_path().to_owned());
        let content_tab_generation = self.desktop_workspace.content_tab_generation;
        let workspace_load_generation = self.desktop_workspace.workspace_load_generation;
        let area_status =
            storage::DesktopAreaStatusStore::new(self.database_paths.app_db_path().to_owned());
        let load_id = id.clone();
        let wallpaper_revision = self.wallpaper_settings.scoped.revision;
        let work = cx.background_executor().spawn(async move {
            store.touch(&load_id)?;
            let areas = store.list_projects(&load_id)?;
            let tabs = content_tabs.list(&load_id)?;
            let content_state = content_tabs.state(&load_id)?;
            let statuses = area_status.list(&load_id)?;
            let unavailable = areas
                .iter()
                .filter_map(|area| {
                    let available = area.root.is_dir()
                        && area.root.canonicalize().is_ok_and(|root| root == area.root);
                    (!available).then(|| {
                        (
                            area.id.clone(),
                            ui::i18n::LocalizedText::new("workspace.project_directory_unavailable"),
                        )
                    })
                })
                .collect();
            Ok::<_, storage::StorageError>((
                store.list()?,
                areas,
                tabs,
                statuses,
                unavailable,
                content_state,
            ))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |app, window, cx| {
                if app.desktop_workspace.active_id.as_deref() != Some(&id)
                    || app.desktop_workspace.workspace_load_generation != workspace_load_generation
                {
                    return;
                }
                match result {
                    Ok((mut workspaces, mut areas, tabs, statuses, unavailable, content_state)) => {
                        app.preserve_wallpaper_snapshot(
                            &mut workspaces,
                            &mut areas,
                            wallpaper_revision,
                            cx,
                        );
                        app.desktop_workspace.workspaces = workspaces;
                        app.desktop_workspace.areas = areas;
                        app.desktop_workspace.workbench.saved = content_state
                            .and_then(|s| serde_json::from_str(&s).ok())
                            .unwrap_or_default();
                        if app.desktop_workspace.content_tab_generation == content_tab_generation {
                            app.desktop_workspace.content_tabs = tabs;
                        }
                        app.desktop_workspace.status_cache = statuses
                            .into_iter()
                            .map(|status| (status.area_id.clone(), status))
                            .collect();
                        app.mark_workspace_activity_dirty(cx);
                        if app.desktop_workspace.drawers.right_open {
                            app.load_selected_desktop_content(cx);
                        }
                        app.desktop_workspace.area_errors = unavailable;
                        app.bind_desktop_wallpaper(cx);
                        app.broadcast_wallpaper_state(None, cx);
                        // Workspace navigation lands in overview. Only an explicit project
                        // action may request expansion, including while these areas load.
                        let area_ids: Vec<_> = app
                            .desktop_workspace
                            .areas
                            .iter()
                            .map(|area| area.id.clone())
                            .collect();
                        for area_id in area_ids {
                            app.ensure_desktop_area_session(area_id, window, cx);
                        }
                    }
                    Err(error) => {
                        app.desktop_workspace.error = Some(ui::i18n::message(
                            "workspace.could_not_open_workspace_error",
                            &[("error", (error).to_string())],
                        ))
                    }
                }
                app.desktop_workspace.busy = false;
                cx.notify();
            });
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }

    pub(super) fn start_desktop_status_refresh(&mut self, cx: &mut Context<Self>) {
        if self.desktop_workspace.refresh_started {
            return;
        }
        self.desktop_workspace.refresh_started = true;
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            if this
                .update(cx, |app, cx| {
                    if !app.window_active {
                        return;
                    }
                    app.refresh_desktop_artifacts_if_changed(cx);
                    if app.desktop_workspace.sessions.is_empty() {
                        return;
                    }
                    app.refresh_desktop_agent_status_cache(cx);
                    if matches!(app.screen, AppScreen::Launchpad)
                        || (matches!(app.screen, AppScreen::DesktopWorkspace)
                            && app.desktop_workspace.expanded_area_id.is_none())
                    {
                        cx.notify();
                    }
                })
                .is_err()
            {
                break;
            }
        })
        .detach();
    }

    pub(super) fn show_desktop_launchpad(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_desktop_directory(true, window, cx);
    }

    pub(super) fn show_desktop_directory(
        &mut self,
        home: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.desktop_workspace.home_directory != home {
            self.desktop_directory_scroll
                .set_offset(gpui::point(gpui::px(0.0), gpui::px(0.0)));
        }
        self.desktop_workspace.home_directory = home;
        if !matches!(self.screen, AppScreen::Launchpad) {
            self.desktop_workspace.directory.selected = self.desktop_workspace.active_id.clone();
        }
        if home {
            self.desktop_workspace.home.search_open = false;
            self.desktop_workspace.search_query.clear();
            self.desktop_workspace.actions_workspace_id = None;
        }
        self.focus_handle.focus(window);
        let project = self
            .desktop_workspace
            .transition
            .as_ref()
            .and_then(|t| match &t.subject {
                super::camera::DesktopCanvasSubject::Project { area_id, .. } => {
                    Some(area_id.clone())
                }
                _ => None,
            })
            .or_else(|| self.desktop_workspace.expanded_area_id.clone());
        self.desktop_workspace.pending_navigation = None;
        if let (Some(workspace), Some(area)) = (self.desktop_workspace.active_id.clone(), project) {
            self.begin_desktop_project_directory_zoom(&workspace, &area, true, window, cx);
            if self.desktop_workspace.transition.is_none() {
                self.detach_visible_desktop_area(cx);
            }
            self.desktop_workspace.expanded_area_id = None;
        } else if !matches!(self.screen, AppScreen::Launchpad)
            || self.desktop_workspace_zoom_active()
        {
            if let Some(id) = self.desktop_workspace.active_id.clone() {
                self.begin_desktop_workspace_zoom(&id, true, window, cx);
            }
        }
        self.save_desktop_overview_scroll(cx);
        self.desktop_workspace.actions_area_id = None;
        self.screen = AppScreen::Launchpad;
        self.bind_desktop_wallpaper(cx);
        self.refresh_desktop_previews(cx);
        cx.notify();
    }

    pub(super) fn detach_visible_desktop_area(&mut self, cx: &mut Context<Self>) {
        self.save_desktop_overview_scroll(cx);
        if let (Some(workspace_id), Some(area_id)) = (
            self.desktop_workspace.active_id.as_ref(),
            self.desktop_workspace.expanded_area_id.as_ref(),
        ) {
            self.desktop_workspace
                .expanded_by_workspace
                .insert(workspace_id.clone(), Some(area_id.clone()));
            if let Some(session) = self
                .desktop_workspace
                .sessions
                .get(&(workspace_id.clone(), area_id.clone()))
            {
                session.update(cx, |session, cx| session.detach_project_window(cx));
            }
        }
    }

    fn save_desktop_overview_scroll(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        if let Some(handle) = self.desktop_overview_scroll.get(&id) {
            handle.set_offset(gpui::point(gpui::px(0.0), gpui::px(0.0)));
        }
        if self
            .desktop_workspace
            .active()
            .is_none_or(|workspace| workspace.overview_scroll_y == 0)
        {
            return;
        }
        let offset = 0;
        if let Some(workspace) = self
            .desktop_workspace
            .workspaces
            .iter_mut()
            .find(|item| item.id == id)
        {
            workspace.overview_scroll_y = offset;
        }
        let store = self.desktop_workspace.store.clone();
        let previous = self.desktop_workspace.scroll_save_task.take();
        self.desktop_workspace.scroll_save_task = Some(cx.spawn(async move |this, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            let result = cx
                .background_executor()
                .spawn(async move { store.set_overview_scroll_y(&id, offset) })
                .await;
            let _ = this.update(cx, |app, cx| {
                if let Err(error) = result {
                    app.desktop_workspace.error = Some(ui::i18n::message(
                        "workspace.could_not_save_workspace_position_error",
                        &[("error", (error).to_string())],
                    ));
                    cx.notify();
                }
            });
        }));
    }

    pub(super) fn persist_desktop_view(
        &mut self,
        workspace_id: String,
        area_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.desktop_workspace
            .expanded_by_workspace
            .insert(workspace_id.clone(), area_id.clone());
        let store = self.desktop_workspace.store.clone();
        let previous = self.desktop_workspace.view_save_task.take();
        self.desktop_workspace.view_save_task = Some(cx.spawn(async move |this, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            let work = cx
                .background_executor()
                .spawn(async move { store.set_expanded_area(&workspace_id, area_id.as_deref()) });
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                if let Err(error) = result {
                    app.desktop_workspace.error = Some(ui::i18n::message(
                        "workspace.could_not_save_workspace_view_error",
                        &[("error", (error).to_string())],
                    ));
                    cx.notify();
                }
            });
        }));
    }
}
