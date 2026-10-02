use super::DesktopDeleteTarget;
use crate::app::MultisoulApp;
use gpui::{Context, EntityId, Window};
use storage::DesktopProjectArea;

fn validate_desktop_agent_root(area: &DesktopProjectArea) -> Result<(), String> {
    if !area.root.is_dir() {
        return Err("Project directory is unavailable".into());
    }
    let identity =
        workspace::project::identify_project(&area.root).map_err(|error| error.to_string())?;
    if identity.root != area.root || identity.key != area.project_key {
        return Err("Project directory or repository identity changed".into());
    }
    Ok(())
}

impl MultisoulApp {
    pub(crate) fn begin_desktop_agent_creation(
        &mut self,
        source: EntityId,
        profile: canvas::AgentProfile,
        definition_id: Option<i64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(area_id) = self
            .desktop_workspace
            .sessions
            .iter()
            .find(|((workspace_id, _), session)| {
                self.desktop_workspace.active_id.as_ref() == Some(workspace_id)
                    && session.entity_id() == source
            })
            .map(|((_, area_id), _)| area_id.clone())
        else {
            return;
        };
        self.begin_desktop_agent_creation_for_area(area_id, profile, definition_id, window, cx);
    }

    pub(super) fn begin_desktop_agent_creation_for_area(
        &mut self,
        area_id: String,
        profile: canvas::AgentProfile,
        definition_id: Option<i64>,
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
            .find(|area| area.id == area_id && area.desktop_workspace_id == workspace_id)
            .cloned()
        else {
            return;
        };
        let key = (workspace_id, area_id);
        let Some(session) = self.desktop_workspace.sessions.get(&key).cloned() else {
            self.desktop_workspace.error =
                Some("Project canvas is still loading. Try again.".into());
            cx.notify();
            return;
        };
        self.desktop_workspace.error = None;
        cx.notify();
        let checked_area = area.clone();
        let work = cx
            .background_executor()
            .spawn(async move { validate_desktop_agent_root(&checked_area) });
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |app, window, cx| {
                let state = &app.desktop_workspace;
                let deleting = match &state.pending_delete {
                    Some(DesktopDeleteTarget::Workspace { id, .. }) => id == &key.0,
                    Some(DesktopDeleteTarget::Area {
                        workspace_id,
                        area_id,
                        ..
                    }) => workspace_id == &key.0 && area_id == &key.1,
                    None => false,
                };
                // Bind completion to the original instance, even after navigation.
                let current = state.sessions.get(&key) == Some(&session)
                    && session.read(cx).runtime.workspace_root() == Some(area.root.as_path())
                    && (state.active_id.as_ref() != Some(&key.0)
                        || state.areas.iter().any(|current| current == &area));
                if deleting || !current {
                    app.desktop_workspace.error = Some(
                        "Project changed or was removed. Select a project and try again.".into(),
                    );
                } else if let Err(error) = result {
                    app.desktop_workspace.error = Some(format!("Could not create Agent: {error}"));
                } else {
                    session.update(cx, |session, cx| {
                        session.spawn_desktop_agent_at(
                            profile,
                            definition_id,
                            &area.root,
                            window,
                            cx,
                        )
                    });
                }
                cx.notify();
            });
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/desktop_agent_creation_tests.rs"]
mod tests;
