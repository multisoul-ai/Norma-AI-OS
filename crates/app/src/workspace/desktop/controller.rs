use crate::app::{AppScreen, MultisoulApp};
use gpui::{AppContext as _, Context, Focusable, Window};
use std::path::PathBuf;
use std::time::Duration;
use ui::component::input::{InputEvent, InputState};

impl MultisoulApp {
    pub(crate) fn refresh_desktop_previews(&mut self, cx: &mut Context<Self>) {
        let store = self.desktop_workspace.store.clone();
        let work = cx
            .background_executor()
            .spawn(async move { store.list_previews() });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| match result {
                Ok(previews) => {
                    app.desktop_workspace.previews = previews;
                    cx.notify();
                }
                Err(error) => {
                    app.desktop_workspace.error =
                        Some(format!("Could not load workspace previews: {error}"));
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
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            host.update(cx, |host, cx| {
                host.accept_project_for_desktop(path, window, cx)
            });
            return;
        }
        if matches!(self.screen, AppScreen::DesktopWorkspace)
            && self.desktop_workspace.active_id.is_some()
        {
            self.add_project_to_desktop_workspace(path, window, cx);
        } else {
            self.desktop_workspace.pending_project_path = Some(path);
            cx.notify();
        }
    }

    pub(super) fn cancel_desktop_project_destination(&mut self, cx: &mut Context<Self>) {
        self.desktop_workspace.pending_project_path = None;
        cx.notify();
    }

    pub(super) fn send_desktop_agent_input(
        &mut self,
        workspace_id: String,
        area_id: String,
        node_id: canvas::NodeId,
        input: gpui::Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.desktop_workspace.active_id.as_deref() != Some(&workspace_id)
            || !self
                .desktop_workspace
                .areas
                .iter()
                .any(|area| area.id == area_id)
            || self.desktop_workspace.area_errors.contains_key(&area_id)
        {
            self.desktop_workspace.error = Some("Project area is unavailable".to_string());
            cx.notify();
            return;
        }
        let text = input.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }
        let Some(session) = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id, area_id))
        else {
            self.desktop_workspace.error = Some("Agent session is still loading".to_string());
            cx.notify();
            return;
        };
        match session.update(cx, |session, cx| {
            session.submit_compact_agent_text(node_id, &text, cx)
        }) {
            Ok(()) => {
                input.update(cx, |input, cx| input.set_value("", window, cx));
                self.desktop_workspace.error = None;
            }
            Err(error) => {
                self.desktop_workspace.error =
                    Some(format!("Could not send Agent message: {error}"))
            }
        }
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
        let Some(path) = self.desktop_workspace.pending_project_path.take() else {
            return;
        };
        self.desktop_workspace.busy = true;
        self.desktop_workspace.error = None;
        cx.notify();
        let store = self.desktop_workspace.store.clone();
        let work = cx.background_executor().spawn(async move {
            let id = match target_id {
                Some(id) => id,
                None => store.create("New Workspace")?.id,
            };
            store.add_project(&id, &path)?;
            Ok::<_, storage::StorageError>((id, store.list()?))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |app, window, cx| {
                app.desktop_workspace.busy = false;
                match result {
                    Ok((id, workspaces)) => {
                        app.desktop_workspace.workspaces = workspaces;
                        app.open_desktop_workspace(id, window, cx);
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
    pub(super) fn create_desktop_workspace(&mut self, cx: &mut Context<Self>) {
        if self.desktop_workspace.busy {
            return;
        }
        self.desktop_workspace.busy = true;
        self.start_desktop_status_refresh(cx);
        self.desktop_workspace.error = None;
        cx.notify();
        let name = format!("Workspace {}", self.desktop_workspace.workspaces.len() + 1);
        let store = self.desktop_workspace.store.clone();
        let work = cx.background_executor().spawn(async move {
            let created = store.create(&name)?;
            let list = store.list()?;
            Ok::<_, storage::StorageError>((created, list))
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                app.desktop_workspace.busy = false;
                match result {
                    Ok((created, list)) => {
                        app.desktop_workspace.workspaces = list;
                        app.desktop_workspace.active_id = Some(created.id);
                        app.desktop_workspace.areas.clear();
                        app.desktop_workspace.active_area_id = None;
                        app.screen = AppScreen::DesktopWorkspace;
                    }
                    Err(error) => {
                        app.desktop_workspace.error =
                            Some(format!("Could not create workspace: {error}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn open_desktop_workspace(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self
            .desktop_workspace
            .workspaces
            .iter()
            .any(|item| item.id == id)
        {
            return;
        }
        self.detach_visible_desktop_area(cx);
        self.start_desktop_status_refresh(cx);
        self.desktop_workspace.active_id = Some(id.clone());
        self.desktop_workspace.areas.clear();
        self.desktop_workspace.active_area_id = None;
        self.desktop_workspace.expanded_area_id = None;
        self.desktop_workspace.agent_creation = None;
        self.close_desktop_switcher(cx);
        self.desktop_workspace.busy = true;
        self.screen = AppScreen::DesktopWorkspace;
        self.desktop_workspace.error = None;
        cx.notify();
        let store = self.desktop_workspace.store.clone();
        let load_id = id.clone();
        let work = cx.background_executor().spawn(async move {
            store.touch(&load_id)?;
            let areas = store.list_projects(&load_id)?;
            let unavailable = areas
                .iter()
                .filter_map(|area| {
                    let available = area.root.is_dir()
                        && area.root.canonicalize().is_ok_and(|root| root == area.root);
                    (!available)
                        .then(|| (area.id.clone(), "Project directory unavailable".to_string()))
                })
                .collect();
            Ok::<_, storage::StorageError>((store.list()?, areas, unavailable))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |app, window, cx| {
                if app.desktop_workspace.active_id.as_deref() != Some(&id) {
                    return;
                }
                match result {
                    Ok((workspaces, areas, unavailable)) => {
                        app.desktop_workspace.workspaces = workspaces;
                        app.desktop_workspace.areas = areas;
                        app.desktop_workspace.area_errors = unavailable;
                        let stored_view = app
                            .desktop_workspace
                            .workspaces
                            .iter()
                            .find(|item| item.id == id)
                            .and_then(|item| item.expanded_area_id.clone());
                        if let Some(area_id) = app
                            .desktop_workspace
                            .expanded_by_workspace
                            .get(&id)
                            .cloned()
                            .unwrap_or(stored_view)
                        {
                            app.expand_desktop_project_area(area_id, window, cx);
                        }
                        let area_ids: Vec<_> = app
                            .desktop_workspace
                            .areas
                            .iter()
                            .map(|area| area.id.clone())
                            .collect();
                        for area_id in area_ids {
                            app.ensure_desktop_area_session(area_id, false, window, cx);
                        }
                    }
                    Err(error) => {
                        app.desktop_workspace.error =
                            Some(format!("Could not open workspace: {error}"))
                    }
                }
                app.desktop_workspace.busy = false;
                cx.notify();
            });
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }

    fn start_desktop_status_refresh(&mut self, cx: &mut Context<Self>) {
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
                    if !app.window_active || app.desktop_workspace.sessions.is_empty() {
                        return;
                    }
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

    pub(super) fn show_desktop_launchpad(&mut self, cx: &mut Context<Self>) {
        self.screen = AppScreen::Launchpad;
        self.refresh_desktop_previews(cx);
        cx.notify();
    }

    pub(super) fn detach_visible_desktop_area(&mut self, cx: &mut Context<Self>) {
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
                    app.desktop_workspace.error =
                        Some(format!("Could not save workspace view: {error}"));
                    cx.notify();
                }
            });
        }));
    }

    pub(crate) fn open_desktop_switcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            host.update(cx, |host, cx| host.open_desktop_switcher(window, cx));
            return;
        }
        if self.desktop_workspace.switcher_open {
            self.close_desktop_switcher(cx);
            return;
        }
        self.refresh_desktop_previews(cx);
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search workspaces"));
        let subscription = cx.subscribe(&input, |app, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                app.desktop_workspace.switcher_query = input.read(cx).value().to_string();
                cx.notify();
            }
        });
        input.focus_handle(cx).focus(window);
        self.desktop_workspace.switcher_input = Some(input);
        self.desktop_switcher_subscription = Some(subscription);
        self.desktop_workspace.switcher_query.clear();
        self.desktop_workspace.switcher_open = true;
        cx.notify();
    }

    pub(crate) fn close_desktop_switcher(&mut self, cx: &mut Context<Self>) {
        self.desktop_workspace.switcher_open = false;
        self.desktop_workspace.switcher_input = None;
        self.desktop_switcher_subscription = None;
        self.desktop_workspace.switcher_query.clear();
        cx.notify();
    }
}
