use super::*;

impl AgentAnnouncements {
    pub(super) fn enqueue_terminal(
        &mut self,
        node_id: NodeId,
        agent_name: Option<&str>,
        outcome: AgentAnnouncementOutcome,
        eligibility: AnnouncementEligibility,
    ) {
        if !eligibility.enabled
            || eligibility.tts_availability == AgentAnnouncementTtsAvailability::Unavailable
        {
            let reason = if !eligibility.enabled {
                "disabled"
            } else {
                "tts_unavailable"
            };
            tracing::info!(
                node_id,
                outcome = outcome.spoken_word(),
                reason,
                "agent announcement skipped"
            );
            return;
        }
        let Some(agent_name) = agent_name.map(str::trim).filter(|name| !name.is_empty()) else {
            tracing::warn!(
                node_id,
                outcome = outcome.spoken_word(),
                reason = "missing_agent_name",
                "agent announcement skipped"
            );
            return;
        };
        if self.pending.len() == MAX_PENDING_ANNOUNCEMENTS {
            let dropped = self.pending.pop_front();
            tracing::warn!(dropped_node_id = ?dropped.map(|pending| pending.announcement.node_id),
                "agent announcement queue is full; dropped the oldest pending announcement");
        }
        let waiting_for_tts =
            eligibility.tts_availability == AgentAnnouncementTtsAvailability::Waiting;
        self.pending.push_back(PendingAgentAnnouncement {
            announcement: AgentAnnouncement::terminal(node_id, agent_name, outcome),
            waiting_for_tts,
        });
        tracing::info!(
            node_id,
            outcome = outcome.spoken_word(),
            waiting_for_tts,
            pending_count = self.pending.len(),
            "agent announcement queued"
        );
    }

    pub(super) fn handle_coordinated_terminal(
        &mut self,
        event: &RuntimeEvent,
        tasks: &[agent::AgentCoordinatedTask],
        eligibility: AnnouncementEligibility,
    ) -> bool {
        let Some(node_id) = runtime_event_node_id(event) else {
            return false;
        };
        let mut handled = false;
        for task in tasks
            .iter()
            .filter(|task| task.target_node_id == node_id && task.state != "cancelled")
        {
            if let Some(outcome) = coordinated_outcome(task.state) {
                handled |= self.report_coordinated_result(task, outcome, eligibility, true);
            }
        }
        if handled {
            return true;
        }
        // Native cards may finish without a Hook-driven coordinator transition.
        // Consume each terminal in order, before a following prompt opens a new round.
        let Some(task) = tasks.iter().find(|task| {
            task.target_node_id == node_id
                && matches!(task.state, "running" | "cancel_requested")
                && !self.reported_tasks.contains(&task.task_id)
        }) else {
            return false;
        };
        let Some(outcome) = observed_terminal(task, event) else {
            return false;
        };
        self.report_coordinated_result(task, outcome, eligibility, true)
    }

    fn report_coordinated_result(
        &mut self,
        task: &agent::AgentCoordinatedTask,
        outcome: AgentAnnouncementOutcome,
        eligibility: AnnouncementEligibility,
        claim_round: bool,
    ) -> bool {
        if !self.reported_tasks.insert(task.task_id) {
            return false;
        }
        if claim_round {
            self.rounds
                .entry(task.target_node_id)
                .or_default()
                .terminal_claimed = true;
        }
        self.enqueue_terminal(task.target_node_id, Some(&task.agent), outcome, eligibility);
        true
    }

    pub(super) fn queue_coordinated_results(
        &mut self,
        tasks: &[agent::AgentCoordinatedTask],
        events: &[RuntimeEvent],
        eligibility: AnnouncementEligibility,
        doubao: bool,
    ) {
        // Coordinator::tasks sorts by task_id; retain only its bounded terminal history.
        self.reported_tasks
            .retain(|id| tasks.binary_search_by_key(id, |task| task.task_id).is_ok());
        for task in tasks {
            let Some(outcome) = coordinated_outcome(task.state) else {
                continue;
            };
            if !doubao {
                self.reported_tasks.insert(task.task_id);
                continue;
            }
            let node_id = task.target_node_id;
            if events.iter().any(|event| matches!(event, RuntimeEvent::NodeRemoved { node_id: removed, .. } if *removed == node_id)) {
                self.reported_tasks.insert(task.task_id);
                continue;
            }
            // A queued cancellation has no card turn. Do not claim another active task's round.
            // A new prompt in this batch also owns a fresh round, separate from the old task result.
            let newer_round = matches!(latest_round_boundary(node_id, events),
                Some(RuntimeEvent::AgentTaskStateChanged { source_event, .. })
                if matches!(source_event.as_str(), "UserPromptSubmit" | "turn/started"));
            self.report_coordinated_result(
                task,
                outcome,
                eligibility,
                task.state != "cancelled" && !newer_round,
            );
        }
    }
}

fn latest_round_boundary(node_id: NodeId, events: &[RuntimeEvent]) -> Option<&RuntimeEvent> {
    events.iter().rev().find(|event| matches!(event,
        RuntimeEvent::AgentTaskStateChanged { node_id: id, to, source_event, .. }
        if *id == node_id && (matches!(source_event.as_str(), "UserPromptSubmit" | "turn/started")
            || matches!(to, AgentTaskState::Done | AgentTaskState::Failed { .. } | AgentTaskState::Ended))
    ))
}

fn observed_terminal(
    task: &agent::AgentCoordinatedTask,
    event: &RuntimeEvent,
) -> Option<AgentAnnouncementOutcome> {
    let RuntimeEvent::AgentTaskStateChanged {
        to, source_event, ..
    } = event
    else {
        return None;
    };
    if *to == AgentTaskState::Ended {
        return Some(AgentAnnouncementOutcome::Interrupted);
    }
    let outcome = AgentAnnouncementOutcome::from_task_event(to, source_event)?;
    Some(
        if outcome == AgentAnnouncementOutcome::Done && task.state == "cancel_requested" {
            AgentAnnouncementOutcome::Interrupted
        } else {
            outcome
        },
    )
}

fn coordinated_outcome(state: &str) -> Option<AgentAnnouncementOutcome> {
    match state {
        "succeeded" => Some(AgentAnnouncementOutcome::Done),
        "failed" => Some(AgentAnnouncementOutcome::Failed),
        "cancelled" => Some(AgentAnnouncementOutcome::Cancelled),
        "interrupted" => Some(AgentAnnouncementOutcome::Interrupted),
        _ => None,
    }
}
