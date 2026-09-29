use super::DesktopAgentCreation;
use crate::app::MultisoulApp;
use gpui::{Context, Window};
use std::path::PathBuf;

impl MultisoulApp {
    pub(crate) fn begin_desktop_agent_creation(
        &mut self,
        profile: canvas::AgentProfile,
        definition_id: Option<i64>,
        cx: &mut Context<Self>,
    ) {
        let (Some(workspace_id), Some(area_id)) = (
            self.desktop_workspace.active_id.clone(),
            self.desktop_workspace.expanded_area_id.clone(),
        ) else {
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
        if self.desktop_workspace.area_errors.contains_key(&area_id) {
            return;
        }
        self.desktop_workspace.agent_creation = Some(DesktopAgentCreation {
            workspace_id: workspace_id.clone(),
            area_id: area_id.clone(),
            profile,
            definition_id,
            choices: Vec::new(),
            loading: true,
        });
        cx.notify();
        let work = cx.background_executor().spawn(async move {
            let identity = workspace::project::identify_project(&area.root)
                .map_err(|error| error.to_string())?;
            if identity.key != area.project_key {
                return Err("Project repository identity changed".to_string());
            }
            workspace::project::project_worktrees(&identity).map_err(|error| error.to_string())
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                if let Some(pending) = app.desktop_workspace.agent_creation.as_mut() {
                    if pending.workspace_id != workspace_id || pending.area_id != area_id {
                        return;
                    }
                    pending.loading = false;
                    match result {
                        Ok(choices) => pending.choices = choices,
                        Err(error) => {
                            app.desktop_workspace.error =
                                Some(format!("Could not list worktrees: {error}"))
                        }
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn cancel_desktop_agent_creation(&mut self, cx: &mut Context<Self>) {
        self.desktop_workspace.agent_creation = None;
        cx.notify();
    }

    pub(super) fn choose_desktop_agent_worktree(
        &mut self,
        worktree: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = self.desktop_workspace.agent_creation.as_ref() else {
            return;
        };
        if pending.loading || !pending.choices.contains(&worktree) {
            return;
        }
        let Some(area) = self
            .desktop_workspace
            .areas
            .iter()
            .find(|area| area.id == pending.area_id)
        else {
            return;
        };
        let project_key = area.project_key.clone();
        let target = (pending.workspace_id.clone(), pending.area_id.clone());
        let profile = pending.profile;
        let definition_id = pending.definition_id;
        let check_path = worktree.clone();
        self.desktop_workspace
            .agent_creation
            .as_mut()
            .expect("pending chooser exists")
            .loading = true;
        cx.notify();
        let work = cx.background_executor().spawn(async move {
            let identity = workspace::project::identify_project(&check_path)
                .map_err(|error| error.to_string())?;
            if identity.key != project_key {
                return Err("Selected worktree no longer belongs to this project".to_string());
            }
            Ok::<_, String>(identity.root)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |app, window, cx| {
                if app.desktop_workspace.active_id.as_deref() != Some(&target.0)
                    || app.desktop_workspace.expanded_area_id.as_deref() != Some(&target.1)
                {
                    app.desktop_workspace.agent_creation = None;
                    cx.notify();
                    return;
                }
                match result {
                    Ok(worktree) => {
                        if let Some(session) = app.desktop_workspace.sessions.get(&target) {
                            session.update(cx, |session, cx| {
                                session.spawn_desktop_agent_at(
                                    profile,
                                    definition_id,
                                    &worktree,
                                    window,
                                    cx,
                                )
                            });
                            app.desktop_workspace.agent_creation = None;
                        }
                    }
                    Err(error) => {
                        if let Some(pending) = app.desktop_workspace.agent_creation.as_mut() {
                            pending.loading = false;
                        }
                        app.desktop_workspace.error =
                            Some(format!("Could not create Agent: {error}"));
                    }
                }
                cx.notify();
            });
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }
}
