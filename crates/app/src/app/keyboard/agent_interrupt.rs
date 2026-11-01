//! Local Agent interruption state for terminal keyboard input.

use crate::app::MultisoulApp;
use canvas::{CanvasNodeKind, NodeId};
use gpui::Context;
use runtime::NodeInput;
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
                | AgentTaskState::WaitingQuestion
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
        app.session_reply_queue.interrupt_node(node_id);
        app.advance_agent_task_queue(node_id);
    }
}

impl MultisoulApp {
    pub(crate) fn interrupt_agent_from_desktop_overview(
        &mut self,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !matches!(
            self.runtime.agent_task_state(node_id),
            Some(
                AgentTaskState::Thinking
                    | AgentTaskState::RunningTool { .. }
                    | AgentTaskState::WaitingApproval { .. }
                    | AgentTaskState::WaitingQuestion
            )
        ) {
            return Err("Agent 任务已结束".into());
        }
        let input = KeyInput {
            key: Key::Char('c'),
            ctrl: true,
            alt: false,
            shift: false,
        };
        self.runtime
            .send_node_input(node_id, NodeInput::Key(input.clone()))
            .map_err(|error| error.to_string())?;
        end_active_agent_task_for_manual_interrupt(self, node_id, &input);
        cx.notify();
        Ok(())
    }
}
