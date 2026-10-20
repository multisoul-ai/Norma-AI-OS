use super::DesktopDeleteTarget;
use crate::app::MultisoulApp;
use gpui::{Context, EntityId, Window};
use storage::DesktopProjectArea;

fn validate_desktop_agent_root(area: &DesktopProjectArea) -> Result<(), String> {
    if !area.root.is_dir() {
        return Err(ui::i18n::text("workspace.project_directory_is_unavailable").into());
    }
    let identity =
        workspace::project::identify_project(&area.root).map_err(|error| error.to_string())?;
    if identity.root != area.root || identity.key != area.project_key {
        return Err(
            ui::i18n::text("workspace.project_directory_or_repository_identity_changed").into(),
        );
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
            tracing::warn!(active = ?self.desktop_workspace.active_id, sessions = self.desktop_workspace.sessions.len(), "[DEBUG-a7c3] agent creation: source session not found");
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
            tracing::warn!("[DEBUG-a7c3] agent creation: no active workspace");
            return;
        };
        let Some(area) = self
            .desktop_workspace
            .areas
            .iter()
            .find(|area| area.id == area_id && area.desktop_workspace_id == workspace_id)
            .cloned()
        else {
            tracing::warn!(%area_id, "[DEBUG-a7c3] agent creation: area not in active areas");
            return;
        };
        let key = (workspace_id, area_id);
        let Some(session) = self.desktop_workspace.sessions.get(&key).cloned() else {
            tracing::warn!(?key, "[DEBUG-a7c3] agent creation: session missing for key");
            self.desktop_workspace.error = Some(ui::i18n::LocalizedText::new(
                "workspace.project_canvas_is_still_loading_try_again",
            ));
            cx.notify();
            return;
        };
        tracing::info!(?profile, area = %key.1, "[DEBUG-a7c3] agent creation: validating directory");
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
                tracing::info!(
                    deleting,
                    current,
                    ok = result.is_ok(),
                    "[DEBUG-a7c3] agent creation: validation done"
                );
                if deleting || !current {
                    tracing::warn!(
                        same_session = state.sessions.get(&key) == Some(&session),
                        root_ok =
                            session.read(cx).runtime.workspace_root() == Some(area.root.as_path()),
                        area_in_active = state.areas.iter().any(|c| c == &area),
                        "[DEBUG-a7c3] agent creation: current check failed"
                    );
                    app.desktop_workspace.error = Some(ui::i18n::LocalizedText::new(
                        "workspace.project_changed_or_was_removed_select_a_project_and_try_again",
                    ));
                } else if let Err(error) = result {
                    tracing::warn!(%error, "[DEBUG-a7c3] agent creation: validation failed");
                    app.desktop_workspace.error = Some(ui::i18n::message(
                        "workspace.could_not_create_agent_error",
                        &[("error", (error).to_string())],
                    ));
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
