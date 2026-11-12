use norma::agent_hook::{CompletionAction, CompletionJudge};
use runtime::{AgentTaskState, AgentTaskUpdate, Runtime};

use super::{
    apply_agent_hook_event_with_outcome, canvas_profile, task_result_from_hook_event,
    AgentHookDrainReport, AgentHookServer, AgentHookStartedTask, AgentHookTerminalTask, ProbeNote,
};

impl AgentHookServer {
    pub(super) fn drain_at(
        &self,
        runtime: &mut Runtime,
        project_id: Option<&str>,
        now_ms: u64,
    ) -> AgentHookDrainReport {
        let mut report = AgentHookDrainReport::default();
        if let Some(project_id) = project_id {
            report.changed |= runtime.drain_native_question_hosts(project_id);
            report.session_binding_changed |= runtime.take_native_question_session_changed();
        }
        for (event, outcome) in runtime.take_native_agent_task_updates() {
            report.changed |= runtime.observe_native_question_result(&event);
            self.publish_accepted_event(&mut report, event, false, false, outcome);
        }
        self.abandon_dead_agents(runtime);
        self.ingest_process_probes(runtime, now_ms, &mut report);
        for event in self.receiver.try_iter() {
            if !event_matches_card_profile(runtime, &event) || !runtime.accepts_staff_hook(&event) {
                continue;
            }
            let actions = self.completion.borrow_mut().observe(&event, now_ms);
            self.apply_completion_actions(&mut report, runtime, actions, now_ms);
        }
        let due = self.completion.borrow_mut().poll(now_ms);
        self.apply_completion_actions(&mut report, runtime, due, now_ms);
        while let Ok(approval) = self.approvals.try_recv() {
            let mut event = approval.request().event.clone();
            event.control_run_id = Some(approval.request().run_id.clone());
            if runtime.accept_mobile_approval(approval) {
                report.changed = true;
                report.events.push(event.clone());
                apply_agent_hook_event_with_outcome(runtime, event);
            }
        }
        report.changed |= runtime.settle_delivered_hook_answers();
        for question in self.questions.try_iter() {
            let Some(project_id) = project_id else {
                question.cancel();
                continue;
            };
            match runtime.accept_native_question(question, project_id) {
                Ok(_) => report.changed = true,
                Err(reason) => tracing::debug!(reason, "native question registration rejected"),
            }
        }
        report
    }

    fn abandon_dead_agents(&self, runtime: &Runtime) {
        let nodes = self.completion.borrow().tracked_nodes();
        for node_id in nodes {
            let Some(pid) = runtime.agent_child_pid(node_id) else {
                continue;
            };
            if !runtime::agent_process_alive(pid) {
                self.completion.borrow_mut().abandon(node_id);
            }
        }
    }

    fn ingest_process_probes(
        &self,
        runtime: &mut Runtime,
        now_ms: u64,
        report: &mut AgentHookDrainReport,
    ) {
        while let Ok(note) = self.probe_rx.try_recv() {
            let actions =
                self.completion
                    .borrow_mut()
                    .note_process_probe(note.node_id, note.found, now_ms);
            self.apply_completion_actions(report, runtime, actions, now_ms);
        }
    }

    fn apply_completion_actions(
        &self,
        report: &mut AgentHookDrainReport,
        runtime: &mut Runtime,
        actions: Vec<CompletionAction>,
        _now_ms: u64,
    ) {
        for action in actions {
            match action {
                CompletionAction::Forward(event) => {
                    self.publish_mapped_event(report, runtime, event, false, false);
                }
                CompletionAction::KeepThinking { event } => {
                    self.publish_keep_thinking(report, runtime, &event);
                }
                CompletionAction::Confirm {
                    event,
                    background_unsettled,
                } => {
                    self.publish_mapped_event(report, runtime, event, background_unsettled, true);
                }
                CompletionAction::ProbeProcesses {
                    node_id,
                    since_unix_ms,
                } => self.spawn_descendant_probe(runtime, node_id, since_unix_ms),
            }
        }
    }

    fn publish_keep_thinking(
        &self,
        report: &mut AgentHookDrainReport,
        runtime: &mut Runtime,
        event: &norma::agent_hook::AgentHookEvent,
    ) {
        // KeepThinking 不经 observe_mobile_hook，对应的 Confirm 若被门禁拒收会静默丢弃。
        // 所以它只能保持进行中的回合，不能把已结束或已换会话的卡片拉回运行中。
        if !matches!(
            runtime.agent_task_state(event.node_id),
            Some(
                AgentTaskState::Thinking
                    | AgentTaskState::RunningTool { .. }
                    | AgentTaskState::WaitingApproval { .. }
                    | AgentTaskState::WaitingQuestion
            )
        ) {
            return;
        }
        let outcome = runtime.apply_agent_task_update_with_outcome(AgentTaskUpdate {
            node_id: event.node_id,
            profile: canvas_profile(event.profile),
            provider_event: "BackgroundPending".to_string(),
            to: AgentTaskState::Thinking,
            task_title: None,
            session_id: event.session_id.clone(),
        });
        if outcome.changed() {
            report.changed = true;
            report.session_binding_changed |= outcome.session_binding_changed;
        }
    }

    fn publish_mapped_event(
        &self,
        report: &mut AgentHookDrainReport,
        runtime: &mut Runtime,
        event: norma::agent_hook::AgentHookEvent,
        background_unsettled: bool,
        force_changed: bool,
    ) {
        if !runtime.accepts_staff_hook(&event) {
            return;
        }
        report.changed |= runtime.observe_native_question_result(&event);
        let previous_progress = runtime.mobile_task_progress(event.node_id);
        if !runtime.observe_mobile_hook(&event) {
            return;
        }
        report.changed |= previous_progress != runtime.mobile_task_progress(event.node_id);
        let outcome = apply_agent_hook_event_with_outcome(runtime, event.clone());
        self.publish_accepted_event(report, event, background_unsettled, force_changed, outcome);
    }

    fn publish_accepted_event(
        &self,
        report: &mut AgentHookDrainReport,
        event: norma::agent_hook::AgentHookEvent,
        background_unsettled: bool,
        force_changed: bool,
        outcome: runtime::AgentTaskUpdateOutcome,
    ) {
        #[cfg(feature = "e2e-test-support")]
        if let Some(tap) = &self.event_tap {
            let _ = tap.try_send(event.clone());
        }
        let node_id = event.node_id;
        let prompt = norma::agent_hook::extract_prompt_text(&event.raw_json);
        let started_task = (event.provider_event == "UserPromptSubmit"
            && !norma::agent_hook::user_prompt_is_only_settled_task_notification(&event.raw_json))
        .then_some(AgentHookStartedTask {
            node_id,
            task_title: prompt,
        });
        let state = super::agent_task_state_from_hook_event(&event);
        let is_ready = matches!(state, Some(AgentTaskState::Ready));
        let terminal_task = match state.clone() {
            Some(
                settled_state @ (AgentTaskState::Done
                | AgentTaskState::Failed { .. }
                | AgentTaskState::Ended),
            ) => Some(AgentHookTerminalTask {
                node_id,
                settled_state,
                result: task_result_from_hook_event(&event, background_unsettled),
            }),
            _ => None,
        };
        report.events.push(event.clone());
        if let Some(started_task) = started_task {
            report
                .queue_effects
                .push(super::AgentHookQueueEffect::Started(started_task.clone()));
            report.started_tasks.push(started_task);
        }
        if outcome.changed() || force_changed {
            report.changed = true;
            report.session_binding_changed |= outcome.session_binding_changed;
            if outcome.task_state_changed {
                if is_ready {
                    report.ready_node_ids.push(node_id);
                }
                if let Some(terminal_task) = terminal_task {
                    report
                        .queue_effects
                        .push(super::AgentHookQueueEffect::Terminal(terminal_task.clone()));
                    report.terminal_tasks.push(terminal_task);
                }
            }
        }
    }

    fn spawn_descendant_probe(&self, runtime: &Runtime, node_id: u64, since_unix_ms: u64) {
        let Some(pid) = runtime.agent_child_pid(node_id) else {
            return;
        };
        let sender = self.probe_tx.clone();
        let started = std::thread::Builder::new()
            .name("agent-descendant-probe".to_string())
            .spawn(move || {
                let found = runtime::has_agent_turn_descendant(pid, since_unix_ms / 1000);
                let _ = sender.try_send(ProbeNote { node_id, found });
            });
        if started.is_err() {
            tracing::debug!(node_id, "agent descendant probe thread did not start");
        }
    }
}

/// Grok 会兼容读取 ~/.claude/settings.json、~/.cursor/hooks.json，同一事件以其他来源多转发几次。
/// 它们共用 node 的判定账本，必须在入口丢弃，否则后到的副本会覆盖暂存的 Stop。
fn event_matches_card_profile(
    runtime: &Runtime,
    event: &norma::agent_hook::AgentHookEvent,
) -> bool {
    match runtime.document.node(event.node_id).map(|node| &node.kind) {
        Some(canvas::CanvasNodeKind::Agent(spec)) => spec.profile == canvas_profile(event.profile),
        _ => true,
    }
}

pub(super) fn fresh_judge() -> std::cell::RefCell<CompletionJudge> {
    std::cell::RefCell::new(CompletionJudge::new())
}

pub(super) fn unix_now_ms() -> u64 {
    u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis())
            .unwrap_or(0),
    )
    .unwrap_or(u64::MAX)
}
