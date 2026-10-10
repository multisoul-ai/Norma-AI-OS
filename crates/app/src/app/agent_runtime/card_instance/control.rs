use super::*;
use runtime::{AgentTaskState, NodeInput};

impl MultisoulApp {
    pub fn cancel_unsubmitted_card_instance_assignment(
        &mut self,
        id: &str,
    ) -> Result<CardInstanceReceipt, CardInstanceDeliveryError> {
        let receipt = self
            .card_instance_receipt(id)
            .ok_or_else(|| delivery_error("assignment_missing", true))?;
        if self.card_instance_tasks.transports.contains_key(id) {
            return Err(delivery_error("input_already_started", false));
        }
        if !matches!(
            receipt.stage,
            CardInstanceStage::Accepted | CardInstanceStage::WaitingInput
        ) {
            return Err(delivery_error("already_submitted", true));
        }
        self.card_instance_tasks
            .coordinator
            .cancel_unsubmitted(id)
            .map_err(|_| delivery_error("not_cancellable", true))?;
        self.card_instance_receipt(id)
            .ok_or_else(|| delivery_error("assignment_missing", true))
    }

    /// Never interrupts a merely queued or previously completed task, or the card's newer direct turn.
    pub fn interrupt_card_instance_assignment(
        &mut self,
        id: &str,
    ) -> Result<CardInstanceReceipt, CardInstanceDeliveryError> {
        let assignment = self
            .card_instance_tasks
            .coordinator
            .assignment(id)
            .cloned()
            .ok_or_else(|| delivery_error("assignment_missing", true))?;
        let receipt = self
            .card_instance_receipt(id)
            .ok_or_else(|| delivery_error("assignment_missing", true))?;
        if receipt.stage != CardInstanceStage::Running {
            return Err(delivery_error("not_running", true));
        }
        if receipt.cancel_requested {
            return Ok(receipt);
        }
        let turn = receipt
            .bound_turn
            .as_ref()
            .ok_or_else(|| delivery_error("turn_unknown", true))?;
        self.validate_card_instance_assignment(&assignment, true)?;
        let node = assignment.target.card.node_id;
        let latest = self.runtime.mobile_latest_turn_key(node);
        let matches = latest
            .as_ref()
            .map(|key| {
                turn.run_id.as_deref() == Some(&key.run_id)
                    && turn.session_id == key.session_id
                    && turn.turn_id == key.turn_id
            })
            .unwrap_or_else(|| {
                self.card_instance_tasks
                    .latest_turns
                    .get(&assignment.target.card)
                    == Some(turn)
            });
        if !matches {
            return Err(delivery_error("turn_changed", true));
        }
        if !matches!(
            self.runtime.agent_task_state(node),
            Some(
                AgentTaskState::Thinking
                    | AgentTaskState::RunningTool { .. }
                    | AgentTaskState::WaitingApproval { .. }
            )
        ) {
            return Err(delivery_error("not_running", true));
        }
        if self.runtime.workspace_card_user_input_pending(node)
            || self.runtime.agent_task_input_held(node)
            || self.pending_agent_submissions.has_pending_for_node(node)
            || self.session_reply_queue.has_pending_for_node(node)
        {
            return Err(delivery_error("desktop_input_pending", true));
        }
        if self.runtime.workspace_card_uses_rpc(node) {
            let run = turn
                .run_id
                .as_deref()
                .ok_or_else(|| delivery_error("turn_unknown", true))?;
            match self
                .runtime
                .interrupt_mobile_codex(node, run, &turn.turn_id)
            {
                Ok(receiver) => {
                    self.card_instance_tasks
                        .interrupts
                        .insert(id.into(), receiver);
                }
                Err(_) => {
                    self.fail_card_instance_transport(id, "outcome_unknown", false);
                    return Err(delivery_error("outcome_unknown", false));
                }
            }
        } else if self
            .runtime
            .send_node_input(
                node,
                NodeInput::Key(terminal::KeyInput {
                    key: terminal::Key::Char('c'),
                    ctrl: true,
                    alt: false,
                    shift: false,
                }),
            )
            .is_err()
        {
            self.fail_card_instance_transport(id, "outcome_unknown", false);
            return Err(delivery_error("outcome_unknown", false));
        }
        // Sending Ctrl-C is a request, never proof that the task has ended.
        self.card_instance_tasks
            .coordinator
            .request_cancel(id)
            .map_err(|_| delivery_error("not_cancellable", true))?;
        self.card_instance_receipt(id)
            .ok_or_else(|| delivery_error("assignment_missing", true))
    }
}

impl MultisoulApp {
    pub(super) fn poll_card_instance_interrupts(&mut self) -> bool {
        let ids: Vec<_> = self
            .card_instance_tasks
            .interrupts
            .keys()
            .cloned()
            .collect();
        let mut changed = false;
        for id in ids {
            let result = self.card_instance_tasks.interrupts[&id].try_recv();
            if matches!(result, Err(crossbeam_channel::TryRecvError::Empty)) {
                continue;
            }
            self.card_instance_tasks.interrupts.remove(&id);
            changed = true;
            let Some(receipt) = self.card_instance_receipt(&id) else {
                continue;
            };
            let Some(turn) = receipt.bound_turn else {
                continue;
            };
            match result {
                Ok(Ok(value)) if value["turnId"].as_str() == Some(turn.turn_id.as_str()) => {
                    let stage = match value["status"].as_str() {
                        Some("completed") => CardInstanceStage::Succeeded,
                        Some("failed") => CardInstanceStage::Failed,
                        Some("interrupted") => CardInstanceStage::Interrupted,
                        _ => {
                            self.fail_card_instance_transport(&id, "outcome_unknown", false);
                            continue;
                        }
                    };
                    self.record_card_instance_terminal_evidence(
                        &id,
                        &receipt.target,
                        &turn,
                        stage,
                        None,
                    );
                }
                _ => self.fail_card_instance_transport(&id, "outcome_unknown", false),
            }
        }
        changed
    }
}
