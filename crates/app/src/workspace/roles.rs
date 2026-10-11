//! Role operations identify cards, while settings identify reusable configurations.
mod creation;
mod editor;
use crate::app::MultisoulApp;
use agent::workspace_dispatch::WorkspaceCardRef;
use gpui::Context;
use storage::WorkspaceAgentRole;

pub(crate) fn project_role_title(
    runtime: &mut runtime::Runtime,
    role: &WorkspaceAgentRole,
) -> bool {
    let Some(node) = runtime
        .document
        .node_mut(role.card.node_id)
        .filter(|n| n.card_instance_id == role.card.card_instance_id)
    else {
        return false;
    };
    let canvas::CanvasNodeKind::Agent(spec) = &mut node.kind else {
        return false;
    };
    if spec.alias.as_deref() == Some(&role.name) {
        return false;
    }
    spec.alias = Some(role.name.clone());
    true
}

impl MultisoulApp {
    pub(crate) fn workspace_role_scope(&self) -> Option<(&str, &str)> {
        let rest = self.workspace_id.as_deref()?.strip_prefix("desktop:")?;
        rest.split_once(':')
    }

    pub(crate) fn workspace_card_ref(&self, node_id: u64) -> Option<WorkspaceCardRef> {
        let (workspace, area) = self.workspace_role_scope()?;
        let node = self.runtime.document.node(node_id)?;
        matches!(node.kind, canvas::CanvasNodeKind::Agent(_)).then(|| WorkspaceCardRef {
            workspace_id: workspace.into(),
            area_id: area.into(),
            node_id,
            card_instance_id: node.card_instance_id.clone(),
        })
    }

    pub(crate) fn project_workspace_role_titles(
        &mut self,
        roles: &[WorkspaceAgentRole],
        cx: &mut Context<Self>,
    ) {
        for role in roles {
            if let Some(session) = self
                .desktop_workspace
                .sessions
                .get(&(role.card.workspace_id.clone(), role.card.area_id.clone()))
            {
                session.update(cx, |app, cx| {
                    if project_role_title(&mut app.runtime, role) {
                        app.mark_canvas_dirty();
                        cx.notify();
                    }
                });
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/workspace_role_publication_tests.rs"]
mod tests;
