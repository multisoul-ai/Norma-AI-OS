//! Local Agent interruption state for terminal keyboard input.

use crate::app::MultisoulApp;
use canvas::{CanvasNodeKind, NodeId};
use runtime::{AgentTaskState, AgentTaskUpdate};
use terminal::{Key, KeyInput};

/// Records an explicit Ctrl+C interruption before the caller forwards ETX to the Agent PTY.
pub(super) fn end_active_agent_task_for_manual_interrupt(
    app: &mut MultisoulApp,
    node_id: NodeId,
    input: &KeyInput,
) {
    let Key::Char(key) = input.key else {
        return;
    };
    if !input.ctrl || !key.eq_ignore_ascii_case(&'c') {
        return;
    }
    let Some(CanvasNodeKind::Agent(agent)) =
        app.runtime.document.node(node_id).map(|node| &node.kind)
    else {
        return;
    };
    if !matches!(
        app.runtime.agent_task_state(node_id),
        Some(
            AgentTaskState::Thinking
                | AgentTaskState::RunningTool { .. }
                | AgentTaskState::WaitingApproval { .. }
        )
    ) {
        return;
    }
    let profile = agent.profile;
    if app.runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id,
        profile,
        provider_event: "ManualInterrupt".to_string(),
        to: AgentTaskState::Ended,
        task_title: None,
        session_id: None,
    }) {
        app.advance_agent_task_queue(node_id);
    }
}
