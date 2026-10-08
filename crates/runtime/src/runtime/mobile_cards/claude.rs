use super::*;
use std::{collections::VecDeque, sync::mpsc};
use storage::{ApprovalAuditEntry, ApprovalAuditStore, BackgroundApprovalAudit, StorageResult};

type Receipt = (
    Vec<(String, ApprovalDecision)>,
    mpsc::Receiver<StorageResult<()>>,
);

struct TrackedApproval {
    hook: HookApproval,
    recorded: Option<ApprovalStatus>,
}

pub(super) struct ClaudeApprovals {
    approvals: Vec<TrackedApproval>,
    writer: BackgroundApprovalAudit,
    receipts: VecDeque<Receipt>,
    sequence: u64,
}

impl ClaudeApprovals {
    /// Opens SQLite; preparation must be on the background executor.
    pub(super) fn new(store: ApprovalAuditStore) -> Result<Self, String> {
        Ok(Self {
            approvals: Vec::new(),
            writer: BackgroundApprovalAudit::new(store).map_err(|e| e.to_string())?,
            receipts: VecDeque::new(),
            sequence: 0,
        })
    }

    fn accept(&mut self, hook: HookApproval) -> Result<bool, String> {
        if let Err(error) = self.observe() {
            hook.cancel();
            return Err(error);
        }
        if self.approvals.iter().any(|p| p.hook.id() == hook.id()) {
            return Ok(false);
        }
        // Same maximum as Hook ingress; completed tickets are removed after recording.
        if self.approvals.len() >= 64 {
            hook.cancel();
            return Ok(false);
        }
        self.approvals.push(TrackedApproval {
            hook,
            recorded: None,
        });
        self.observe()?;
        Ok(true)
    }

    fn pending(&self) -> Vec<HookApproval> {
        self.approvals
            .iter()
            .filter(|p| p.hook.status(Instant::now()) == ApprovalStatus::Pending)
            .map(|p| p.hook.clone())
            .collect()
    }

    fn choose(&mut self, id: &str, decision: ApprovalDecision) -> Result<bool, String> {
        let Some(p) = self.approvals.iter_mut().find(|p| p.hook.id() == id) else {
            return Ok(false);
        };
        if !p.hook.choose(decision, Instant::now()) {
            return Ok(false);
        }
        // Transport can cancel immediately after choose. Record the accepted intent first,
        // then observe cancellation, so the audit cannot lose an accepted user decision.
        p.recorded = Some(ApprovalStatus::Chosen(decision));
        let hook = p.hook.clone();
        self.record(vec![(hook, ApprovalStatus::Chosen(decision))])?;
        self.observe()?;
        Ok(true)
    }

    fn record(&mut self, changes: Vec<(HookApproval, ApprovalStatus)>) -> Result<(), String> {
        if changes.is_empty() {
            return Ok(());
        }
        // Match the existing writer's 8 batches; stalled SQLite cannot grow the receipt queue.
        if self.receipts.len() >= 8 {
            return Err("Claude audit receipts full".into());
        }
        let mut choices = Vec::new();
        let mut entries = Vec::new();
        for (hook, state) in changes {
            self.sequence = self
                .sequence
                .checked_add(1)
                .ok_or("Claude audit sequence exhausted")?;
            if let ApprovalStatus::Chosen(decision) = state {
                choices.push((hook.id().into(), decision));
            }
            let mut payload =
                serde_json::json!({"id":hook.id(),"request":hook.request(),"state":state});
            if let ApprovalStatus::Chosen(decision)
            | ApprovalStatus::Decided(decision)
            | ApprovalStatus::Responded(decision) = state
            {
                payload["provider_response"] =
                    norma::agent_hook::claude_permission_output(decision);
            }
            entries.push(ApprovalAuditEntry {
                run_id: hook.request().run_id.clone(),
                sequence: self.sequence,
                node_id: hook.request().event.node_id,
                provider: "claude".into(),
                request_id: hook.id().into(),
                payload,
            });
        }
        let receipt = self.writer.append(entries).map_err(|e| e.to_string())?;
        self.receipts.push_back((choices, receipt));
        Ok(())
    }

    pub(super) fn record_turn(
        &mut self,
        outcome: storage::MobileTurnOutcome,
    ) -> Result<(), String> {
        if self.receipts.len() >= 8 {
            return Err("Claude turn outcome receipts full".into());
        }
        let receipt = self
            .writer
            .record_turn(outcome)
            .map_err(|e| e.to_string())?;
        self.receipts.push_back((Vec::new(), receipt));
        Ok(())
    }

    pub(super) fn record_transcript(
        &mut self,
        key: storage::MobileSessionKey,
        path: PathBuf,
    ) -> Result<(), String> {
        if self.receipts.len() >= 8 {
            return Err("Claude session reference receipts full".into());
        }
        let receipt = self
            .writer
            .bind_session_transcript(key, path)
            .map_err(|e| e.to_string())?;
        self.receipts.push_back((Vec::new(), receipt));
        Ok(())
    }

    fn observe(&mut self) -> Result<(), String> {
        let changes: Vec<_> = self
            .approvals
            .iter()
            .filter_map(|p| {
                let status = p.hook.status(Instant::now());
                (p.recorded != Some(status)).then(|| (p.hook.clone(), status))
            })
            .collect();
        self.record(changes.clone())?;
        for (hook, status) in changes {
            if let Some(p) = self.approvals.iter_mut().find(|p| p.hook.id() == hook.id()) {
                p.recorded = Some(status);
            }
        }
        self.approvals.retain(|p| {
            !matches!(
                p.recorded,
                Some(ApprovalStatus::Responded(_) | ApprovalStatus::Cancelled)
            )
        });
        Ok(())
    }

    fn drive(&mut self) -> Result<(), String> {
        // Observe revocation/timeout before consuming a late commit; never restore stale choices.
        self.observe()?;
        while let Some((_, receipt)) = self.receipts.front() {
            match receipt.try_recv() {
                Ok(Ok(())) => {
                    let (choices, _) = self.receipts.pop_front().expect("front exists");
                    for (id, decision) in choices {
                        if let Some(p) = self.approvals.iter_mut().find(|p| p.hook.id() == id) {
                            if p.hook.submit_chosen(decision, Instant::now()) {
                                // Capture Decided even if transport consumes it before the next poll.
                                p.recorded = Some(ApprovalStatus::Decided(decision));
                                let hook = p.hook.clone();
                                self.record(vec![(hook, ApprovalStatus::Decided(decision))])?;
                            }
                        }
                    }
                }
                Ok(Err(error)) => return Err(format!("Claude audit commit failed: {error}")),
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("Claude audit writer disconnected".into())
                }
                Err(mpsc::TryRecvError::Empty) => break,
            }
        }
        self.observe()
    }

    pub(super) fn cancel(&mut self) {
        for p in &self.approvals {
            p.hook.cancel();
        }
        // Enqueue cancellation only. Closing never waits for disk and never sends decisions.
        if let Err(error) = self.observe() {
            tracing::warn!(%error, "Claude cancellation audit unavailable");
        }
    }
}

impl Drop for ClaudeApprovals {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl Runtime {
    pub fn accepts_mobile_hook(&self, event: &norma::agent_hook::AgentHookEvent) -> bool {
        if self
            .mobile_runs
            .get(&event.node_id)
            .is_some_and(MobileRun::is_native)
        {
            return true;
        }
        if !matches!(self.document.node(event.node_id).map(|node| &node.kind), Some(CanvasNodeKind::Agent(spec)) if spec.mobile_control)
        {
            return true;
        }
        if matches!(self.document.node(event.node_id).map(|node| &node.kind), Some(CanvasNodeKind::Agent(spec)) if spec.profile == AgentProfile::Codex)
        {
            return false;
        }
        self.mobile_runs.get(&event.node_id).is_some_and(|run| {
            run.run_id.is_some()
                && run.run_id == event.control_run_id
                && run.session_id == event.session_id
        })
    }

    /// Claude 2.1.196+ supplies prompt_id. Never attach a permission to the next turn.
    pub fn observe_mobile_hook(&mut self, event: &norma::agent_hook::AgentHookEvent) -> bool {
        if !self.accepts_mobile_hook(event) {
            return false;
        }
        self.track_native_agent(event.node_id);
        if self
            .mobile_runs
            .get(&event.node_id)
            .is_some_and(MobileRun::is_native)
        {
            return self.observe_native_hook(event);
        }
        let Some(run) = self.mobile_runs.get_mut(&event.node_id) else {
            return true;
        };
        let Ok(path) = history::hook_path(run, event) else {
            return false;
        };
        match event.provider_event.as_str() {
            "SessionStart" => {
                if run.prompt_id.is_some() {
                    return false;
                }
                run.native_input.acknowledge_start();
            }
            "UserPromptSubmit" => {
                if run.prompt_id.is_some() && run.prompt_id == event.turn_id {
                    return false;
                }
                if let Some(claude) = &mut run.claude {
                    claude.cancel();
                }
                run.finish_terminal_interrupt(Err(
                    "Claude turn replaced before interrupt confirmation".into(),
                ));
                run.prompt_id = event.turn_id.clone();
                run.latest_prompt_id = event.turn_id.clone();
                run.retry_task.observe_hook(event);
                run.accept_mobile_message(event);
                run.native_input.acknowledge();
            }
            "SessionEnd" => {
                run.finish_terminal_interrupt(Err(
                    "Claude session ended before interrupt confirmation".into(),
                ));
                if let Some(claude) = &mut run.claude {
                    claude.cancel();
                }
                run.prompt_id = None;
            }
            _ => {
                if event.turn_id != run.prompt_id || run.prompt_id.is_none() {
                    return false;
                }
                if matches!(event.provider_event.as_str(), "Stop" | "StopFailure") {
                    // Freeze the final reply before the audit writer sees the outcome.
                    run.task_summary.observe_hook(event);
                    let status = if event.provider_event == "Stop" {
                        "completed"
                    } else {
                        "failed"
                    };
                    let state = if status == "completed" {
                        storage::MobileTurnState::Completed
                    } else {
                        storage::MobileTurnState::Failed
                    };
                    if let Err(error) = run.record_claude_turn(event.node_id, state) {
                        tracing::warn!(%error, "Claude turn outcome not queued");
                        run.control.cancel();
                        return false;
                    }
                    run.finish_terminal_interrupt(Ok(
                        serde_json::json!({"turnId":event.turn_id,"status":status}),
                    ));
                    if let Some(claude) = &mut run.claude {
                        claude.cancel();
                    }
                    run.prompt_id = None;
                }
            }
        }
        if let Some(path) = path {
            if let Err(error) = run.record_transcript(event.node_id, path) {
                tracing::warn!(%error, "Claude transcript reference not queued");
                run.control.cancel();
                return false;
            }
        }
        run.task_clock.observe_hook(event);
        run.task_steps.observe(event);
        run.task_summary.observe_hook(event);
        true
    }

    pub fn accept_mobile_approval(&mut self, approval: HookApproval) -> bool {
        let request = approval.request();
        let id = request.event.node_id;
        let valid = self.mobile_runs.get(&id).is_some_and(|run| {
            !run.control.is_cancelled()
                && run.run_id.as_deref() == Some(&request.run_id)
                && run.session_id == request.event.session_id
                && run.prompt_id.is_some()
                && run.prompt_id == request.event.turn_id
                && run.claude.is_some()
                && run.interrupt.is_none()
        }) && !matches!(
            self.agent_task_state(id),
            Some(
                crate::AgentTaskState::Done
                    | crate::AgentTaskState::Failed { .. }
                    | crate::AgentTaskState::Ended
            )
        ) && self
            .live
            .get(&id)
            .is_some_and(|live| live.terminal_status() == Some(TerminalStatus::Running))
            && approval.status(Instant::now()) == ApprovalStatus::Pending;
        if !valid {
            approval.cancel();
            return false;
        }
        let result = self
            .mobile_runs
            .get_mut(&id)
            .and_then(|r| r.claude.as_mut())
            .expect("validated Claude run")
            .accept(approval);
        self.claude_result(id, result)
    }

    pub fn mobile_approvals(&self, node_id: NodeId) -> Vec<HookApproval> {
        self.mobile_runs
            .get(&node_id)
            .and_then(|r| r.claude.as_ref())
            .map(ClaudeApprovals::pending)
            .unwrap_or_default()
    }

    /// Success reserves a choice. Only the background audit commit permits Hook submission.
    pub fn decide_mobile_approval(
        &mut self,
        node_id: NodeId,
        run_id: &str,
        request_id: &str,
        decision: ApprovalDecision,
    ) -> bool {
        let Some(run) = self.mobile_runs.get_mut(&node_id).filter(|r| {
            r.run_id.as_deref() == Some(run_id)
                && !r.control.is_cancelled()
                && r.interrupt.is_none()
        }) else {
            return false;
        };
        let Some(claude) = run.claude.as_mut() else {
            return false;
        };
        let result = claude.choose(request_id, decision);
        self.claude_result(node_id, result)
    }

    fn claude_result(&mut self, id: NodeId, result: Result<bool, String>) -> bool {
        match result {
            Ok(value) => value,
            Err(message) => {
                self.end_mobile_run(id);
                if let Some(live) = self.live.get_mut(&id) {
                    live.kill();
                }
                self.events.update_agent_task_state(
                    id,
                    crate::AgentTaskState::Failed { message },
                    "approval/auditFailed".into(),
                );
                false
            }
        }
    }

    pub(crate) fn drain_mobile_claude(&mut self) -> bool {
        let mut dirty = false;
        let failed: Vec<_> = self
            .mobile_runs
            .iter_mut()
            .filter_map(|(id, run)| {
                dirty |= run.expire_terminal_interrupt();
                let claude = run.claude.as_mut()?;
                if run.control.is_cancelled() {
                    claude.cancel();
                    return None;
                }
                claude.drive().err().map(|error| (*id, error))
            })
            .collect();
        dirty |= !failed.is_empty();
        for (id, error) in failed {
            self.claude_result(id, Err(error));
        }
        dirty
    }
}
