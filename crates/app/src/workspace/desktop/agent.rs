use super::DesktopDeleteTarget;
use crate::app::MultisoulApp;
use gpui::{Context, Entity, EntityId, Window};
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
        let Some((key, session)) = self
            .desktop_workspace
            .sessions
            .iter()
            .find(|((workspace_id, _), session)| {
                self.desktop_workspace.active_id.as_ref() == Some(workspace_id)
                    && session.entity_id() == source
            })
            .map(|(key, session)| (key.clone(), session.clone()))
        else {
            return;
        };
        let Some(area) = self
            .desktop_workspace
            .areas
            .iter()
            .find(|area| area.desktop_workspace_id == key.0 && area.id == key.1)
            .cloned()
        else {
            return;
        };
        // The source session is still borrowed by its action. Freeze its target before deferring.
        cx.defer_in(window, move |host, window, cx| {
            host.begin_desktop_agent_creation_for_target(
                area,
                session,
                profile,
                definition_id,
                window,
                cx,
            );
        });
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
            self.desktop_workspace.error = Some(ui::i18n::LocalizedText::new(
                "workspace.project_canvas_is_still_loading_try_again",
            ));
            cx.notify();
            return;
        };
        self.begin_desktop_agent_creation_for_target(
            area,
            session,
            profile,
            definition_id,
            window,
            cx,
        );
    }

    fn begin_desktop_agent_creation_for_target(
        &mut self,
        area: DesktopProjectArea,
        session: Entity<Self>,
        profile: canvas::AgentProfile,
        definition_id: Option<i64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = (area.desktop_workspace_id.clone(), area.id.clone());
        if self.desktop_workspace.sessions.get(&key) != Some(&session) {
            return;
        }
        let reserved = session.update(cx, |app, _| {
            let spec = match definition_id {
                Some(id) => {
                    let definition = app
                        .agent_definitions
                        .definitions()
                        .iter()
                        .find(|d| d.id == id)?;
                    canvas::AgentSpec {
                        profile: definition.runtime_profile,
                        alias: Some(definition.name.clone()),
                        definition_id: Some(id),
                        mobile_control: false,
                    }
                }
                None => canvas::AgentSpec {
                    profile,
                    alias: None,
                    definition_id: None,
                    mobile_control: false,
                },
            };
            let canvas = app.persistence.project_store()?.canvas_store();
            let bounds = app.next_tiled_node_bounds(window);
            let node = app
                .runtime
                .document
                .reserve_node(canvas::CanvasNodeKind::Agent(spec), bounds);
            let publication = app.persistence.begin_card_publication();
            Some((
                node,
                canvas,
                app.runtime.runtime_instance_id().to_owned(),
                publication,
            ))
        });
        let Some((mut node, canvas, runtime_instance, publication)) = reserved else {
            return;
        };
        let card = agent::workspace_dispatch::WorkspaceCardRef {
            workspace_id: key.0.clone(),
            area_id: key.1.clone(),
            node_id: node.id,
            card_instance_id: node.card_instance_id.clone(),
        };
        self.desktop_workspace.error = None;
        cx.notify();
        let checked_area = area.clone();
        let roles =
            storage::WorkspaceAgentRoleStore::new(self.database_paths.app_db_path().to_owned());
        let catalog = self.desktop_workspace.store.clone();
        let work = cx.background_executor().spawn(async move {
            let node_id = node.id;
            let identity = node.card_instance_id.clone();
            let result = (|| {
                validate_desktop_agent_root(&checked_area)?;
                if !catalog
                    .list_projects(&card.workspace_id)
                    .map_err(|e| e.to_string())?
                    .contains(&checked_area)
                {
                    return Err("项目已移除".into());
                }
                roles
                    .register_workspace(&card.workspace_id)
                    .map_err(|e| e.to_string())?;
                let role = roles
                    .materialize_workspace_card(&card, &canvas, &node, false)
                    .map_err(|e| e.to_string())?;
                if let canvas::CanvasNodeKind::Agent(spec) = &mut node.kind {
                    spec.alias = Some(role.name);
                }
                Ok::<_, String>(node)
            })();
            let absent = matches!(canvas.contains_card_instance(node_id, &identity), Ok(false));
            (result, absent)
        });
        cx.spawn_in(window, async move |this, cx| {
            let (result, absent) = work.await;
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
                let current = state.sessions.get(&key) == Some(&session)
                    && session.read(cx).runtime.workspace_root() == Some(area.root.as_path())
                    && session.read(cx).runtime.runtime_instance_id() == runtime_instance;
                let installed = session.update(cx, |session, cx| {
                    let mut installed = false;
                    if let Ok(node) = result.as_ref() {
                        // A persisted card is installed before any whole-document writer resumes.
                        if current && !deleting {
                            session.install_registered_agent_card(node.clone(), Some(window), cx);
                            installed = session
                                .runtime
                                .document
                                .node(node.id)
                                .is_some_and(|n| n.card_instance_id == node.card_instance_id);
                        }
                    }
                    session
                        .persistence
                        .finish_card_publication(&publication, installed, absent);
                    installed
                });
                let result = result.and_then(|_| {
                    if installed {
                        Ok(())
                    } else {
                        Err("创建结果待确认，请重新打开项目以恢复已保存的卡片".into())
                    }
                });
                if let Err(error) = result {
                    app.desktop_workspace.error = Some(
                        ui::i18n::LocalizedText::new("common.operation_failed")
                            .with_arg("reason", error),
                    );
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
