use super::*;

pub(super) async fn cleanup(
    session: &Entity<MultisoulApp>,
    runtime_instance: &str,
    node: Option<u64>,
    definition: &AgentDefinition,
    store: &AgentDefinitionStore,
    cx: &mut gpui::AsyncApp,
) {
    let id = definition.id;
    if let Some(node) = node {
        let _=session.update(cx,|app,cx| {
            if app.runtime.runtime_instance_id()!=runtime_instance{return;}
            if matches!(app.runtime.document.node(node).map(|n|&n.kind),Some(canvas::CanvasNodeKind::Agent(s)) if s.definition_id==Some(id)) {
                if let Some(card)=app.workspace_card_ref(node){app.persistence.queue_role_removal(storage::WorkspaceAgentRoleStore::new(app.database_paths.app_db_path().to_owned()),card);}
                app.runtime.remove_node(node);app.mark_canvas_dirty();app.mark_session_dirty();cx.notify();
            }
        });
    }
    // Configurations are shared. Removing this role must not delete another card's definition.
    if session
        .read_with(cx, |app, _| app.workspace_role_scope().is_some())
        .unwrap_or(true)
    {
        return;
    }
    let store = store.clone();
    if cx
        .background_executor()
        .spawn(async move { store.delete_if_quick(id) })
        .await
        .is_ok()
    {
        let _ = session.update(cx, |app, cx| {
            app.agent_definitions.forget_created_definition(id);
            cx.notify();
        });
    }
}
