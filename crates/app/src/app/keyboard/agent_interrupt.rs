//! Local Agent interruption state for terminal keyboard input.

use canvas::{CanvasNodeKind, NodeId};
use runtime::{AgentTaskState, AgentTaskUpdate, Runtime};
use terminal::{Key, KeyInput};

/// Records an explicit Ctrl+C interruption before the caller forwards ETX to the Agent PTY.
pub(super) fn end_active_agent_task_for_manual_interrupt(
    runtime: &mut Runtime,
    node_id: NodeId,
    input: &KeyInput,
) {
    let Key::Char(key) = input.key else {
        return;
    };
    if !input.ctrl || !key.eq_ignore_ascii_case(&'c') {
        return;
    }
    let Some(CanvasNodeKind::Agent(agent)) = runtime.document.node(node_id).map(|node| &node.kind)
    else {
        return;
    };
    if !matches!(
        runtime.agent_task_state(node_id),
        Some(
            AgentTaskState::Thinking
                | AgentTaskState::RunningTool { .. }
                | AgentTaskState::WaitingApproval { .. }
        )
    ) {
        return;
    }
    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id,
        profile: agent.profile,
        provider_event: "ManualInterrupt".to_string(),
        to: AgentTaskState::Ended,
        task_title: None,
        session_id: None,
    });
}
