use super::DesktopDeleteTarget;
use crate::app::{AppScreen, MultisoulApp};
use gpui::{AppContext as _, Context, Focusable, Window};
use ui::component::input::{InputEvent, InputState};

#[derive(Clone)]
pub(crate) enum DesktopRenameTarget {
    Workspace(String),
    Area {
        workspace_id: String,
        area_id: String,
    },
}

impl MultisoulApp {
    pub(super) fn begin_desktop_rename(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.desktop_workspace.busy {
            return;
        }
        let (target, name) = if let Some(workspace) = self
            .desktop_workspace
            .workspaces
            .iter()
            .find(|item| item.id == id)
        {
            (DesktopRenameTarget::Workspace(id), workspace.name.clone())
        } else if let Some(area) = self
            .desktop_workspace
            .areas
            .iter()
            .find(|area| area.id == id)
        {
            (
                DesktopRenameTarget::Area {
                    workspace_id: area.desktop_workspace_id.clone(),
                    area_id: id,
                },
                area.display_name.clone(),
            )
        } else {
            return;
        };
        let input = cx.new(|cx| InputState::new(window, cx).default_value(name));
        cx.subscribe(&input, |app, _, event, cx| match event {
            InputEvent::PressEnter { .. } => app.save_desktop_rename(cx),
            InputEvent::Change => {
                app.desktop_workspace.error = None;
                cx.notify();
            }
            _ => {}
        })
        .detach();
        input.focus_handle(cx).focus(window);
        self.desktop_workspace.rename_target = Some(target);
        self.desktop_workspace.rename_input = Some(input);
        self.desktop_workspace.error = None;
        cx.notify();
    }

    pub(super) fn cancel_desktop_rename(&mut self, cx: &mut Context<Self>) {
        if self.desktop_workspace.busy {
            return;
        }
        self.desktop_workspace.rename_target = None;
        self.desktop_workspace.rename_input = None;
        self.desktop_workspace.error = None;
        cx.notify();
    }

    pub(super) fn save_desktop_rename(&mut self, cx: &mut Context<Self>) {
        if self.desktop_workspace.busy {
            return;
        }
        let (Some(target), Some(input)) = (
            self.desktop_workspace.rename_target.clone(),
            self.desktop_workspace.rename_input.as_ref(),
        ) else {
            return;
        };
        let name = input.read(cx).value().to_string();
        let name = match storage::DesktopWorkspaceStore::validate_name(&name) {
            Ok(name) => name.to_owned(),
            Err(error) => {
                self.desktop_workspace.error = Some(ui::i18n::message(
                    "workspace.could_not_rename_canvas_error",
                    &[("error", error.to_string())],
                ));
                cx.notify();
                return;
            }
        };
        let store = self.desktop_workspace.store.clone();
        self.desktop_workspace.busy = true;
        cx.notify();
        let work = cx.background_executor().spawn(async move {
            let areas = match &target {
                DesktopRenameTarget::Workspace(id) => {
                    store.rename(id, &name)?;
                    None
                }
                DesktopRenameTarget::Area {
                    workspace_id,
                    area_id,
                } => {
                    store.rename_project(workspace_id, area_id, &name)?;
                    Some((workspace_id.clone(), store.list_projects(workspace_id)?))
                }
            };
            Ok::<_, storage::StorageError>((store.list()?, areas))
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                app.desktop_workspace.busy = false;
                match result {
                    Ok((workspaces, areas)) => {
                        app.desktop_workspace.workspaces = workspaces;
                        if let Some((workspace_id, areas)) = areas {
                            if let Some(previews) =
                                app.desktop_workspace.previews.get_mut(&workspace_id)
                            {
                                for preview in previews {
                                    if let Some(area) =
                                        areas.iter().find(|area| area.id == preview.id)
                                    {
                                        preview.name = area.display_name.clone();
                                    }
                                }
                            }
                            if app.desktop_workspace.active_id.as_deref() == Some(&workspace_id) {
                                app.desktop_workspace.areas = areas;
                            }
                        }
                        app.desktop_workspace.rename_target = None;
                        app.desktop_workspace.rename_input = None;
                        app.desktop_workspace.error = None;
                    }
                    Err(error) => {
                        app.desktop_workspace.error = Some(ui::i18n::message(
                            "workspace.could_not_rename_canvas_error",
                            &[("error", (error).to_string())],
                        ))
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn begin_desktop_area_remove(&mut self, area_id: String, cx: &mut Context<Self>) {
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let Some(area) = self
            .desktop_workspace
            .areas
            .iter()
            .find(|area| area.id == area_id)
        else {
            return;
        };
        self.desktop_workspace.pending_delete = Some(DesktopDeleteTarget::Area {
            workspace_id,
            area_id,
            name: area.display_name.clone(),
        });
        cx.notify();
    }

    pub(super) fn begin_desktop_workspace_delete(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(workspace) = self
            .desktop_workspace
            .workspaces
            .iter()
            .find(|workspace| workspace.id == id)
        else {
            return;
        };
        self.desktop_workspace.pending_delete = Some(DesktopDeleteTarget::Workspace {
            id,
            name: workspace.name.clone(),
        });
        cx.notify();
    }

    pub(super) fn cancel_desktop_delete(&mut self, cx: &mut Context<Self>) {
        self.desktop_workspace.pending_delete = None;
        cx.notify();
    }

    pub(super) fn desktop_delete_running_count(&self, cx: &gpui::App) -> usize {
        let Some(target) = self.desktop_workspace.pending_delete.as_ref() else {
            return 0;
        };
        self.desktop_workspace
            .sessions
            .iter()
            .filter(|((workspace_id, area_id), _)| match target {
                DesktopDeleteTarget::Workspace { id, .. } => workspace_id == id,
                DesktopDeleteTarget::Area {
                    workspace_id: id,
                    area_id: target_area,
                    ..
                } => workspace_id == id && area_id == target_area,
            })
            .map(|(_, session)| session.read(cx).runtime.running_terminal_count())
            .sum()
    }

    pub(super) fn confirm_desktop_delete(&mut self, cx: &mut Context<Self>) {
        if self.desktop_workspace.busy {
            return;
        }
        let Some(target) = self.desktop_workspace.pending_delete.clone() else {
            return;
        };
        self.desktop_workspace.busy = true;
        self.desktop_workspace.error = None;
        cx.notify();
        let store = self.desktop_workspace.store.clone();
        let work = cx.background_executor().spawn(async move {
            let removed_areas = match &target {
                DesktopDeleteTarget::Workspace { id, .. } => {
                    store.delete_catalog(id)?.ok_or_else(|| {
                        anyhow::anyhow!(ui::i18n::text("workspace.workspace_no_longer_exists"))
                    })?
                }
                DesktopDeleteTarget::Area {
                    workspace_id,
                    area_id,
                    ..
                } => {
                    if !store.remove_project_catalog(workspace_id, area_id)? {
                        anyhow::bail!(ui::i18n::text("workspace.project_area_no_longer_exists"));
                    }
                    vec![area_id.clone()]
                }
            };
            Ok::<_, anyhow::Error>((target, removed_areas, store.list()))
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                app.desktop_workspace.busy = false;
                match result {
                    Ok((target, removed_areas, workspaces)) => {
                        let keys: Vec<_> = app.desktop_workspace.sessions.keys()
                            .filter(|(workspace_id, area_id)| match &target {
                                DesktopDeleteTarget::Workspace { id, .. } => workspace_id == id,
                                DesktopDeleteTarget::Area { workspace_id: id, area_id: target_area, .. } => workspace_id == id && area_id == target_area,
                            }).cloned().collect();
                        for key in keys {
                            if let Some(session) = app.desktop_workspace.sessions.remove(&key) {
                                session.update(cx, |session, cx| {
                                    session.stop_project(cx);
                                    session.persistence = Default::default();
                                });
                            }
                        }
                        let (deleted_workspace_id, deleted_area_id) = match &target {
                            DesktopDeleteTarget::Workspace { id, .. } => (id.clone(), None),
                            DesktopDeleteTarget::Area { workspace_id, area_id, .. } => (workspace_id.clone(), Some(area_id.clone())),
                        };
                        app.desktop_workspace.discarded_area_loads.extend(
                            app.desktop_workspace.loading_area_ids.iter()
                                .filter(|(workspace_id, area_id)| workspace_id == &deleted_workspace_id
                                    && deleted_area_id.as_ref().is_none_or(|id| area_id == id))
                                .cloned(),
                        );
                        app.desktop_workspace.pending_delete = None;
                        match workspaces {
                            Ok(workspaces) => app.desktop_workspace.workspaces = workspaces,
                            Err(error) => app.desktop_workspace.error = Some(ui::i18n::message("workspace.could_not_refresh_workspaces_error", &[("error", (error).to_string())])),
                        }
                        match target {
                            DesktopDeleteTarget::Workspace { id, .. } => {
                                app.desktop_workspace.expanded_by_workspace.remove(&id);
                                if app.desktop_workspace.active_id.as_deref() == Some(&id) {
                                    app.desktop_workspace.active_id = None;
                                    app.desktop_workspace.areas.clear();
                                    app.desktop_workspace.expanded_area_id = None;
                                    app.screen = AppScreen::Launchpad;
                                }
                            }
                            DesktopDeleteTarget::Area {
                                workspace_id,
                                area_id,
                                ..
                            } => {
                                app.desktop_workspace
                                    .areas
                                    .retain(|area| area.id != area_id);
                                if app.desktop_workspace.active_area_id.as_deref() == Some(&area_id)
                                {
                                    app.desktop_workspace.active_area_id = None;
                                }
                                if app.desktop_workspace.expanded_area_id.as_deref()
                                    == Some(&area_id)
                                {
                                    app.desktop_workspace.expanded_area_id = None;
                                }
                                if app
                                    .desktop_workspace
                                    .expanded_by_workspace
                                    .get(&workspace_id)
                                    == Some(&Some(area_id.clone()))
                                {
                                    app.desktop_workspace
                                        .expanded_by_workspace
                                        .remove(&workspace_id);
                                }
                            }
                        }
                        let store = app.desktop_workspace.store.clone();
                        let roles=storage::WorkspaceAgentRoleStore::new(app.database_paths.app_db_path().to_owned());
                        cx.background_executor().spawn(async move {
                            for area_id in removed_areas {
                                if let Err(error) = store.remove_canvas_file(&deleted_workspace_id, &area_id) {
                                    tracing::warn!(%error, workspace_id = %deleted_workspace_id, %area_id, "failed to clean deleted desktop canvas");
                                } else if let Err(error)=roles.delete_after_area_removed(&deleted_workspace_id,&area_id) {
                                    tracing::warn!(%error,"failed to release deleted area role names");
                                }
                            }
                        }).detach();
                    }
                    Err(error) => {
                        app.desktop_workspace.error =
                            Some(ui::i18n::message("workspace.could_not_delete_desktop_canvas_error", &[("error", (error).to_string())]))
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
