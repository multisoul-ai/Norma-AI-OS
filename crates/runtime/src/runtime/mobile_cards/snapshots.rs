//! Read-only projections for mobile card lists.
use super::*;

impl Runtime {
    /// On-demand display projection; never reads transcripts, drains PTYs, or starts an executor.
    pub fn mobile_card_snapshots(&self) -> Vec<norma::mobile::CardSnapshot> {
        let clip = |text: String| text.chars().take(1024).collect::<String>();
        let mut cards: Vec<_> = self
            .document
            .nodes()
            .iter()
            .filter_map(|node| {
                let CanvasNodeKind::Agent(spec) = &node.kind else {
                    return None;
                };
                use crate::AgentTaskState;
                let preparing = self.mobile_agent_is_preparing(node.id);
                let interrupting = self.mobile_runs.get(&node.id).is_some_and(|run| {
                    run.interrupt.is_some()
                        || run
                            .run_id
                            .as_deref()
                            .is_some_and(|id| self.mobile_codex_interrupt_pending(node.id, id))
                });
                let state = match self.agent_task_state(node.id) {
                    _ if interrupting => "interrupting",
                    _ if preparing => "preparing",
                    Some(AgentTaskState::Ready) => "ready",
                    Some(AgentTaskState::Thinking) => "thinking",
                    Some(AgentTaskState::RunningTool { .. }) => "running_tool",
                    Some(AgentTaskState::WaitingApproval { .. }) => "waiting_approval",
                    Some(AgentTaskState::WaitingQuestion) => "waiting_question",
                    Some(AgentTaskState::Done) => "done",
                    Some(AgentTaskState::Failed { .. }) => "failed",
                    Some(AgentTaskState::Ended) => "ended",
                    None if self.native_prompt_ready(node.id) => "ready",
                    None if self.node_run_state(node.id).is_none() => "idle",
                    None => "unknown",
                };
                Some(norma::mobile::CardSnapshot {
                    node_id: node.id,
                    profile: spec.profile.display_title().into(),
                    title: node.kind.display_title().chars().take(256).collect(),
                    mobile_control: matches!(
                        spec.profile,
                        AgentProfile::Claude | AgentProfile::Codex | AgentProfile::Cursor
                    ) || self
                        .mobile_runs
                        .get(&node.id)
                        .is_some_and(MobileRun::is_native),
                    can_interrupt: self.mobile_runs.get(&node.id).filter(|run| run.native).map(
                        |run| {
                            spec.profile == AgentProfile::Codex
                                || (spec.profile == AgentProfile::Claude
                                    && run.transcript_path.is_some())
                        },
                    ),
                    task_timing: self.mobile_runs.get(&node.id).and_then(|run| {
                        run.codex
                            .as_ref()
                            .map(|host| &host.task_clock)
                            .unwrap_or(&run.task_clock)
                            .snapshot()
                    }),
                    task_progress: self.mobile_task_progress(node.id),
                    task_summary: self.mobile_runs.get(&node.id).and_then(|run| {
                        run.codex
                            .as_ref()
                            .map(|host| &host.task_summary)
                            .unwrap_or(&run.task_summary)
                            .snapshot()
                    }),
                    state: state.into(),
                    run_id: self.mobile_agent_run_id(node.id).map(str::to_owned),
                    session_id: self.agent_session_id(node.id),
                    turn_id: self.mobile_runs.get(&node.id).and_then(|run| {
                        run.codex
                            .as_ref()
                            .and_then(|host| host.turn_id.clone())
                            .or_else(|| run.prompt_id.clone())
                    }),
                    task_title: self.agent_task_title(node.id).map(clip),
                    latest_prompt: self.agent_latest_prompt(node.id).map(clip),
                })
            })
            .collect();
        cards.sort_by_key(|card| card.node_id);
        cards
    }
}
