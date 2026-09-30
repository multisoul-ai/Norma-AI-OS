use super::*;

impl Runtime {
    pub fn mobile_codex_approvals(
        &mut self,
        node_id: NodeId,
    ) -> Result<Vec<CodexApproval>, String> {
        self.drain_mobile_codex();
        self.mobile_runs
            .get_mut(&node_id)
            .and_then(|run| run.codex.as_mut())
            .ok_or("controlled Codex session unavailable")?
            .approvals
            .pending(Instant::now())
    }

    /// Requires an authorized caller; success reserves a choice, not a Provider response.
    /// Runtime sends only after the project audit confirms durable commit and the request is still live.
    pub fn decide_mobile_codex_approval(
        &mut self,
        node_id: NodeId,
        run_id: &str,
        request_id: &str,
        decision: ApprovalDecision,
    ) -> Result<(), String> {
        // Consume Provider resolution first so a stale UI snapshot cannot authorize a known-closed request.
        self.drain_mobile_codex();
        let host = self
            .mobile_runs
            .get_mut(&node_id)
            .filter(|run| run.run_id.as_deref() == Some(run_id))
            .and_then(|run| run.codex.as_mut())
            .ok_or("controlled Codex run unavailable")?;
        host.approvals
            .choose(request_id, decision, Instant::now())?;
        if let Err(error) = host.audit.record(host.approvals.take_changes()) {
            host.control.cancel();
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn drain_mobile_codex(&mut self) -> bool {
        let mut updates = Vec::new();
        let mut disconnected = Vec::new();
        let mut changed = false;
        for (node_id, run) in &mut self.mobile_runs {
            let Some(host) = &mut run.codex else {
                continue;
            };
            let (states, display_changed) = host.drain(run.run_id.as_deref().unwrap_or(""));
            changed |= display_changed;
            for (state, source) in states {
                updates.push((*node_id, state, source));
            }
            match host.approvals.pending(Instant::now()) {
                Ok(pending) if host.turn_id.is_some() => {
                    let state = updates
                        .iter()
                        .rev()
                        .find(|(id, _, _)| id == node_id)
                        .map(|(_, state, _)| state.clone())
                        .or_else(|| self.events.agent_task_state(*node_id));
                    if let Some(approval) = pending.first() {
                        updates.push((
                            *node_id,
                            AgentTaskState::WaitingApproval {
                                tool_name: Some(approval.method.clone()),
                            },
                            "approval/pending".into(),
                        ));
                    } else if host.external_approvals.is_waiting() {
                        updates.push((
                            *node_id,
                            AgentTaskState::WaitingApproval {
                                tool_name: Some("外部审批处理中".into()),
                            },
                            "hook/permissionWaiting".into(),
                        ));
                    } else if matches!(state, Some(AgentTaskState::WaitingApproval { .. })) {
                        updates.push((
                            *node_id,
                            AgentTaskState::Thinking,
                            "approval/noPending".into(),
                        ));
                    }
                }
                Err(message) => {
                    host.control.cancel();
                    updates.push((
                        *node_id,
                        AgentTaskState::Failed { message },
                        "approval/unavailable".into(),
                    ));
                }
                _ => {}
            }
            if let Err(message) = host.poll_interrupt().and_then(|()| host.drive_audit()) {
                host.control.cancel();
                updates.push((
                    *node_id,
                    AgentTaskState::Failed { message },
                    "approval/auditFailed".into(),
                ));
            }
            if !host.connection.is_connected() || host.control.is_cancelled() {
                disconnected.push(*node_id);
            }
        }
        for (id, state, source) in updates {
            if source == "turn/started" {
                if let Some(run) = self.mobile_runs.get_mut(&id) {
                    run.native_input.acknowledge();
                }
            }
            changed |= self.events.update_agent_task_state(id, state, source);
        }
        for id in disconnected {
            if let Some(live) = self.live.get_mut(&id) {
                live.kill();
            }
            self.mobile_runs.remove(&id);
        }
        changed
    }

    pub(in crate::runtime::mobile_cards) fn request_mobile_codex_message(
        &self,
        node_id: NodeId,
        text: &str,
    ) -> Result<Receiver<Result<Value, String>>, String> {
        let host = self
            .mobile_runs
            .get(&node_id)
            .and_then(|r| r.codex.as_ref())
            .ok_or("controlled Codex unavailable")?;
        if host.turn_id.is_some() || host.interrupt.is_some() {
            return Err("state_not_ready".into());
        }
        host.connection.request(
            "turn/start",
            json!({"threadId":host.thread_id,"input":[{"type":"text","text":text}]}),
        )
    }

    pub fn mobile_codex_initial_turn(&self, node_id: NodeId, session: &str) -> Option<&str> {
        self.mobile_runs
            .get(&node_id)?
            .codex
            .as_ref()
            .filter(|host| host.thread_id == session)?
            .initial_turn_id
            .as_deref()
    }

    /// Full Provider history, paginated at 20 turns; caller must authorize the project first.
    pub fn mobile_codex_history(
        &self,
        node_id: NodeId,
        thread_id: &str,
        cursor: Option<String>,
    ) -> Result<Receiver<Result<Value, String>>, String> {
        let host = self
            .mobile_runs
            .get(&node_id)
            .and_then(|run| run.codex.as_ref())
            .filter(|host| host.thread_id == thread_id)
            .ok_or("controlled Codex session unavailable")?;
        host.connection.request(
            "thread/turns/list",
            json!({"threadId":thread_id,"limit":20,"cursor":cursor,"itemsView":"full"}),
        )
    }
}
