use crate::app::MultisoulApp;
use agent::AdapterError;
use norma::connector::CardIdentity;
fn rejected(code: &str) -> AdapterError {
    AdapterError::Rejected(code.into())
}
/// Exact targets bypass name lookup. Revisions are checked immediately before side effects.
pub(super) fn validate(
    app: &MultisoulApp,
    display: &str,
    node_id: Option<u64>,
    expected: Option<&str>,
    identity: Option<&CardIdentity>,
) -> Result<String, AdapterError> {
    let Some(node_id) = node_id else {
        return Ok(display.into());
    };
    let node = app
        .runtime
        .document
        .node(node_id)
        .ok_or_else(|| rejected("card_not_found"))?;
    let canvas::CanvasNodeKind::Agent(spec) = &node.kind else {
        return Err(rejected("card_not_found"));
    };
    if let Some(expected) = expected {
        let identity = identity.ok_or_else(|| rejected("target_changed"))?;
        if identity.node_id != node_id
            || Some(identity.definition_id) != spec.definition_id
            || crate::connector::live::card_revision(app, identity) != expected
        {
            return Err(rejected("target_changed"));
        }
    }
    let definition = app
        .agent_definitions
        .definitions()
        .iter()
        .find(|d| Some(d.id) == spec.definition_id)
        .ok_or_else(|| rejected("card_not_found"))?;
    if !definition.enabled {
        return Err(rejected("agent_rejected"));
    }
    Ok(definition.name.clone())
}
pub(super) fn dispatch_preflight(
    app: &MultisoulApp,
    node: u64,
    allow_queue: bool,
) -> Result<(), AdapterError> {
    if matches!(
        app.runtime.agent_task_state(node),
        Some(runtime::AgentTaskState::WaitingApproval { .. })
    ) {
        return Err(rejected("approval_pending"));
    }
    if app.runtime.agent_task_input_pending(node) {
        return Err(rejected("input_occupied"));
    }
    let definition = app.runtime.document.node(node).and_then(|n| match &n.kind {
        canvas::CanvasNodeKind::Agent(s) => s.definition_id,
        _ => None,
    });
    let roster = app.agent_roster_for_provider();
    let entry = roster
        .iter()
        .find(|r| Some(r.definition_id) == definition)
        .ok_or_else(|| rejected("card_not_found"))?;
    if entry.tracking == agent::AgentTrackingMode::Untracked {
        return Err(rejected("untracked_requires_confirmation"));
    }
    let busy = matches!(
        app.runtime.agent_task_state(node),
        Some(runtime::AgentTaskState::Thinking | runtime::AgentTaskState::RunningTool { .. })
    ) || app
        .agent_task_coordinator
        .as_ref()
        .is_some_and(|c| c.is_busy(entry.definition_id));
    if !allow_queue && busy {
        return Err(rejected("card_busy"));
    }
    Ok(())
}

pub(super) fn targeted_roster(
    app: &MultisoulApp,
    target: Option<u64>,
) -> Vec<agent::AgentRosterEntry> {
    let mut roster = app.agent_roster_for_provider();
    if let Some(node_id) = target {
        let definition = app
            .runtime
            .document
            .node(node_id)
            .and_then(|node| match &node.kind {
                canvas::CanvasNodeKind::Agent(spec) => spec.definition_id,
                _ => None,
            });
        if let Some(entry) = roster
            .iter_mut()
            .find(|r| Some(r.definition_id) == definition)
        {
            entry.active_node_id = Some(node_id);
            entry.busy = matches!(
                app.runtime.agent_task_state(node_id),
                Some(
                    runtime::AgentTaskState::Thinking
                        | runtime::AgentTaskState::RunningTool { .. }
                        | runtime::AgentTaskState::WaitingApproval { .. }
                )
            );
        }
    }
    roster
}
