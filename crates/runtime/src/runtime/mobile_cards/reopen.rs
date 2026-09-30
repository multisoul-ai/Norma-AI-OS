use super::*;
use crate::AgentTaskState;

impl Runtime {
    /// Explicit opt-in for a legacy card; the App checks its own dispatch/reply queues first.
    /// Keep its saved Provider session, but never carry the old process into controlled mode.
    pub fn enable_mobile_agent_control(
        &mut self,
        node_id: NodeId,
        expected_session_id: &str,
    ) -> Result<(), String> {
        let Some(CanvasNodeKind::Agent(spec)) = self.document.node(node_id).map(|node| &node.kind)
        else {
            return Err("目标 Agent 卡片不存在".into());
        };
        if spec.mobile_control
            || !matches!(spec.profile, AgentProfile::Claude | AgentProfile::Codex)
        {
            return Err("卡片已启用手机控制，或 Provider 不支持".into());
        }
        if expected_session_id.is_empty()
            || self.agent_session_id(node_id).as_deref() != Some(expected_session_id)
        {
            return Err("会话已变化，请刷新后重新选择".into());
        }
        let exited = self.live.get(&node_id).is_some_and(|live| {
            matches!(live.terminal_status(), Some(TerminalStatus::Exited { .. }))
        });
        if self
            .mobile_runs
            .get(&node_id)
            .is_some_and(|run| !run.native || run.message.is_some() || run.interrupt.is_some())
            || (!exited
                && matches!(
                    self.agent_task_state(node_id),
                    Some(
                        AgentTaskState::Thinking
                            | AgentTaskState::RunningTool { .. }
                            | AgentTaskState::WaitingApproval { .. }
                    )
                ))
        {
            return Err("请等待当前任务结束后再重开".into());
        }
        if self
            .live
            .get(&node_id)
            .is_some_and(|live| live.terminal_status() == Some(TerminalStatus::Running))
            && !matches!(
                self.agent_task_state(node_id),
                Some(
                    AgentTaskState::Ready
                        | AgentTaskState::Done
                        | AgentTaskState::Failed { .. }
                        | AgentTaskState::Ended
                )
            )
        {
            return Err("尚未确认 Agent 处于空闲状态".into());
        }
        self.end_mobile_run(node_id);
        if let Some(mut live) = self.live.remove(&node_id) {
            live.kill();
        }
        let CanvasNodeKind::Agent(spec) = &mut self
            .document
            .node_mut(node_id)
            .expect("validated card")
            .kind
        else {
            unreachable!()
        };
        spec.mobile_control = true;
        self.events
            .update_from_terminal_status(node_id, TerminalStatus::Exited { code: None });
        Ok(())
    }
}
