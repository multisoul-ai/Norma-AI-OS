use super::*;
use crossbeam_channel::TryRecvError;
use std::collections::HashSet;
use std::time::Duration;

impl MultisoulApp {
    /// Memory-only advancement, called by the production frame loop. The supplied clock permits deterministic PTY tests.
    pub(crate) fn advance_card_instance_tasks_at(&mut self, now: Instant) -> bool {
        let assignments = self.card_instance_tasks.coordinator.pending_assignments();
        let mut visited = HashSet::new();
        let mut changed = self.poll_card_instance_interrupts();
        for assignment in assignments {
            let id = assignment.assignment_id.clone();
            let node = assignment.target.card.node_id;
            if !visited.insert(node) {
                continue;
            }
            let wrote = self.card_instance_tasks.transports.contains_key(&id);
            if let Err(error) = self.validate_card_instance_assignment(&assignment, !wrote) {
                self.fail_card_instance_transport(&id, &error.code, error.zero_input_proven);
                changed = true;
                continue;
            }
            if let Some(transport) = self.card_instance_tasks.transports.remove(&id) {
                match transport {
                    Transport::Pasted { enter_at } if now < enter_at => {
                        self.card_instance_tasks
                            .transports
                            .insert(id, Transport::Pasted { enter_at });
                    }
                    Transport::Pasted { .. } => {
                        match self
                            .runtime
                            .enter_workspace_card_input(&assignment.target, &id)
                        {
                            Ok(()) => {
                                let _ = self.card_instance_tasks.coordinator.submitted(&id);
                            }
                            Err(error) => self.fail_card_instance_transport(&id, error.code, false),
                        }
                        changed = true;
                    }
                    Transport::Rpc(receiver) => match receiver.try_recv() {
                        Ok(Ok(value)) => {
                            let valid = value["run_id"].as_str()
                                == assignment.target.execution.run_id.as_deref()
                                && value["session_id"].as_str()
                                    == assignment.target.execution.session_id.as_deref()
                                && value["turn_id"].as_str().is_some_and(|id| !id.is_empty());
                            if valid {
                                let _ = self.card_instance_tasks.coordinator.submitted(&id);
                                let turn = CardInstanceTurn {
                                    run_id: assignment.target.execution.run_id.clone(),
                                    session_id: value["session_id"]
                                        .as_str()
                                        .expect("validated session")
                                        .into(),
                                    turn_id: value["turn_id"]
                                        .as_str()
                                        .expect("validated turn")
                                        .into(),
                                };
                                self.card_instance_tasks.coordinator.observe_start(
                                    &assignment.target,
                                    &assignment.text,
                                    turn,
                                );
                            } else {
                                self.fail_card_instance_transport(
                                    &id,
                                    "provider_receipt_changed",
                                    false,
                                );
                            }
                            changed = true;
                        }
                        Ok(Err(_)) | Err(TryRecvError::Disconnected) => {
                            self.fail_card_instance_transport(&id, "outcome_unknown", false);
                            changed = true;
                        }
                        Err(TryRecvError::Empty) => {
                            self.card_instance_tasks
                                .transports
                                .insert(id, Transport::Rpc(receiver));
                        }
                    },
                }
                continue;
            }
            if self.pending_agent_submissions.has_pending_for_node(node)
                || self.session_reply_queue.has_pending_for_node(node)
            {
                let _ = self
                    .card_instance_tasks
                    .coordinator
                    .waiting(&id, "other_input_pending");
                continue;
            }
            if self.runtime.workspace_card_uses_rpc(node) {
                let Some(snapshot) = self
                    .runtime
                    .mobile_card_snapshots()
                    .into_iter()
                    .find(|c| c.node_id == node)
                else {
                    self.fail_card_instance_transport(&id, "target_missing", true);
                    changed = true;
                    continue;
                };
                match self
                    .runtime
                    .send_workspace_message(&snapshot, assignment.text.clone())
                {
                    Ok(receiver) => {
                        self.card_instance_tasks
                            .transports
                            .insert(id.clone(), Transport::Rpc(receiver));
                        let _ = self
                            .card_instance_tasks
                            .coordinator
                            .waiting(&id, "provider_ack_pending");
                        changed = true;
                    }
                    Err(code) if waiting_reason(&code) => {
                        let _ = self.card_instance_tasks.coordinator.waiting(&id, &code);
                    }
                    Err(code) => {
                        self.fail_card_instance_transport(&id, &code, rpc_zero_input_reason(&code));
                        changed = true;
                    }
                }
                continue;
            }
            match self.runtime.reserve_workspace_card_input(
                &assignment.target,
                &id,
                &assignment.text,
            ) {
                Ok(()) => match self.runtime.paste_workspace_card_input(
                    &assignment.target,
                    &id,
                    &assignment.text,
                ) {
                    Ok(()) => {
                        let _ = self
                            .card_instance_tasks
                            .coordinator
                            .waiting(&id, "draft_pasted_enter_pending");
                        // 100 ms preserves the existing CLI paste-to-Enter delay without waiting for another task's terminal state.
                        self.card_instance_tasks.transports.insert(
                            id,
                            Transport::Pasted {
                                enter_at: now + Duration::from_millis(100),
                            },
                        );
                        changed = true;
                    }
                    Err(error) => {
                        self.fail_card_instance_transport(&id, error.code, error.zero_input_proven);
                        changed = true;
                    }
                },
                Err(error) if waiting_reason(error.code) => {
                    let _ = self
                        .card_instance_tasks
                        .coordinator
                        .waiting(&id, error.code);
                }
                Err(error) => {
                    self.fail_card_instance_transport(&id, error.code, error.zero_input_proven);
                    changed = true;
                }
            }
        }
        changed
    }
}
fn waiting_reason(code: &str) -> bool {
    matches!(
        code,
        "input_owned"
            | "desktop_input_pending"
            | "waiting_approval"
            | "terminal_not_ready"
            | "state_not_ready"
            | "terminal_controlled"
            | "workspace_input_pending"
    )
}

fn rpc_zero_input_reason(code: &str) -> bool {
    matches!(
        code,
        "invalid_message"
            | "not_found"
            | "mobile_control_required"
            | "card_changed"
            | "session_changed"
            | "controlled Codex unavailable"
    )
}
