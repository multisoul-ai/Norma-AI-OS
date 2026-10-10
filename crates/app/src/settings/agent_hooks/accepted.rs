use super::{AgentHookDrainReport, AgentHookEvent, Runtime};
use norma::workspace_dispatch::WorkspaceCardTarget;
use std::{collections::HashSet, ops::Deref};

/// Captures identity immediately after this event passes the Runtime guard, before later batch events mutate it.
#[derive(Debug)]
pub(crate) struct AcceptedAgentHookEvent {
    pub(crate) event: AgentHookEvent,
    target: Option<WorkspaceCardTarget>,
    provider: Option<String>,
}
impl AcceptedAgentHookEvent {
    pub(super) fn capture(runtime: &Runtime, event: AgentHookEvent) -> Self {
        let provider = runtime
            .document
            .node(event.node_id)
            .and_then(|node| match &node.kind {
                canvas::CanvasNodeKind::Agent(spec) => Some(spec.profile.display_title().into()),
                _ => None,
            });
        // The private snapshot uses no real route. Only concrete execution identity is compared; never used to deliver.
        let target = runtime
            .workspace_card_target("accepted-hook", "accepted-hook", 0, event.node_id)
            .ok();
        Self {
            event,
            target,
            provider,
        }
    }

    pub(crate) fn matches_target(&self, expected: &WorkspaceCardTarget) -> bool {
        let Some(actual) = &self.target else {
            return false;
        };
        self.provider.as_deref() == Some(format!("{:?}", self.event.profile).as_str())
            && actual.card.node_id == expected.card.node_id
            && actual.card.card_instance_id == expected.card.card_instance_id
            && actual.execution.runtime_instance_id == expected.execution.runtime_instance_id
            && actual.execution.terminal_session_id == expected.execution.terminal_session_id
            && actual.execution.run_id == expected.execution.run_id
            && actual.execution.session_id == expected.execution.session_id
    }
}
impl Deref for AcceptedAgentHookEvent {
    type Target = AgentHookEvent;
    fn deref(&self) -> &Self::Target {
        &self.event
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct AgentHookTaskIdentity {
    node_id: u64,
    provider: String,
    provider_event: String,
    session_id: Option<String>,
    pub(crate) turn_id: Option<String>,
    run_id: Option<String>,
}
impl From<&AgentHookEvent> for AgentHookTaskIdentity {
    fn from(event: &AgentHookEvent) -> Self {
        Self {
            node_id: event.node_id,
            provider: format!("{:?}", event.profile),
            provider_event: event.provider_event.clone(),
            session_id: event.session_id.clone(),
            turn_id: event.turn_id.clone(),
            run_id: event.control_run_id.clone(),
        }
    }
}
impl AgentHookDrainReport {
    pub(crate) fn retain_unclaimed_tasks(&mut self, claimed: &HashSet<AgentHookTaskIdentity>) {
        self.started_tasks
            .retain(|task| !claimed.contains(&task.identity));
        self.terminal_tasks
            .retain(|task| !claimed.contains(&task.identity));
    }
}
