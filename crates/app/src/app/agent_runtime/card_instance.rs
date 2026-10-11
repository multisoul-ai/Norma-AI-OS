//! Foreground adapter for existing, frozen card instances. Durable acceptance belongs to the workspace service.
use crate::app::MultisoulApp;
use agent::card_instance::{
    CardInstanceAssignment, CardInstanceCoordinator, CardInstanceEvent, CardInstanceReceipt,
    CardInstanceStage, CardInstanceTurn,
};
use crossbeam_channel::Receiver;
use norma::workspace_dispatch::WorkspaceCardRef;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Instant;

mod claims;
mod control;
mod evidence;
mod persistence;
mod pump;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CardInstanceDeliveryError {
    pub code: String,
    pub zero_input_proven: bool,
}
impl std::fmt::Display for CardInstanceDeliveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.code)
    }
}
impl std::error::Error for CardInstanceDeliveryError {}

struct Scope {
    workspace: String,
    area: String,
    generation: u64,
    route_token: Option<String>,
    authority: Arc<AtomicBool>,
}
enum Transport {
    Pasted { enter_at: Instant },
    Rpc(Receiver<Result<Value, String>>),
}
#[derive(Default)]
pub(crate) struct CardInstanceTasks {
    coordinator: CardInstanceCoordinator,
    scopes: HashMap<String, Scope>,
    transports: HashMap<String, Transport>,
    latest_turns: HashMap<WorkspaceCardRef, CardInstanceTurn>,
    interrupts: HashMap<String, Receiver<Result<Value, String>>>,
    durable: persistence::DurableCardInstanceEvents,
}

impl MultisoulApp {
    /// Caller authorizes and resolves this exact area/Runtime before calling. Does not create a card/PTY or write storage.
    pub fn accept_card_instance_assignment(
        &mut self,
        workspace: &str,
        area: &str,
        generation: u64,
        assignment: CardInstanceAssignment,
    ) -> Result<CardInstanceReceipt, CardInstanceDeliveryError> {
        self.accept_guarded_card_instance_assignment(
            workspace,
            area,
            generation,
            assignment,
            Arc::new(AtomicBool::new(true)),
        )
    }

    /// A shared service can revoke this in-memory authority before any later paste/Enter; no IO is performed here.
    pub fn accept_guarded_card_instance_assignment(
        &mut self,
        workspace: &str,
        area: &str,
        generation: u64,
        assignment: CardInstanceAssignment,
        authority: Arc<AtomicBool>,
    ) -> Result<CardInstanceReceipt, CardInstanceDeliveryError> {
        if !authority.load(Ordering::Acquire) {
            return Err(delivery_error("unauthorized", true));
        }
        if let Some(previous) = self
            .card_instance_tasks
            .coordinator
            .assignment(&assignment.assignment_id)
        {
            return if previous == &assignment {
                self.card_instance_receipt(&assignment.assignment_id)
                    .ok_or_else(|| delivery_error("assignment_missing", true))
            } else {
                Err(delivery_error("assignment_conflict", true))
            };
        }
        self.preflight_card_instance_assignment(workspace, area, generation, &assignment)?;
        let id = assignment.assignment_id.clone();
        let receipt = self
            .card_instance_tasks
            .coordinator
            .accept(assignment)
            .map_err(|e| delivery_error(e.code(), true))?;
        self.card_instance_tasks.scopes.insert(
            id,
            Scope {
                workspace: workspace.into(),
                area: area.into(),
                generation,
                route_token: self.workspace_route_id.clone(),
                authority,
            },
        );
        Ok(receipt)
    }

    /// Pure memory preflight for T06's all-target check before durable atomic acceptance.
    pub fn preflight_card_instance_assignment(
        &self,
        workspace: &str,
        area: &str,
        generation: u64,
        assignment: &CardInstanceAssignment,
    ) -> Result<(), CardInstanceDeliveryError> {
        self.card_instance_tasks
            .coordinator
            .preflight(assignment)
            .map_err(|e| delivery_error(e.code(), true))?;
        self.runtime
            .validate_workspace_card_target(workspace, area, generation, &assignment.target)
            .map_err(|e| delivery_error(e.code(), true))?;
        if assignment.tracking == agent::card_instance::CardInstanceTracking::Managed
            && assignment.target.execution.session_id.is_none()
        {
            return Err(delivery_error("provider_not_ready", true));
        }
        if let Some(id) = assignment.definition_id {
            let spec = self
                .runtime
                .document
                .node(assignment.target.card.node_id)
                .and_then(|n| match &n.kind {
                    canvas::CanvasNodeKind::Agent(s) => Some(s),
                    _ => None,
                })
                .ok_or_else(|| delivery_error("target_missing", true))?;
            if spec.definition_id != Some(id) {
                return Err(delivery_error("configuration_changed", true));
            }
            if !self
                .agent_definitions
                .definitions()
                .iter()
                .any(|d| d.id == id && d.enabled)
            {
                return Err(delivery_error("role_disabled", true));
            }
        }
        if self.untracked_card_instance_busy(assignment) {
            return Err(delivery_error("untracked_requires_force", true));
        }
        Ok(())
    }

    pub fn card_instance_receipt(&self, id: &str) -> Option<CardInstanceReceipt> {
        self.card_instance_tasks.coordinator.receipt(id)
    }
    pub fn take_card_instance_events(&mut self) -> Vec<CardInstanceEvent> {
        self.journal_workspace_card_instance_events();
        let mut events: Vec<_> = self
            .card_instance_tasks
            .durable
            .legacy_events
            .drain(..)
            .collect();
        events.extend(self.card_instance_tasks.coordinator.take_events());
        events
    }

    /// The shared service supplies a verified durable Provider outcome for the original frozen turn.
    /// This cannot retarget by name or update a merely submitted followup from card-level state.
    pub fn record_card_instance_terminal_evidence(
        &mut self,
        id: &str,
        target: &norma::workspace_dispatch::WorkspaceCardTarget,
        turn: &CardInstanceTurn,
        stage: CardInstanceStage,
        result: Option<norma::protocol::AgentTaskResult>,
    ) -> bool {
        if !self
            .card_instance_receipt(id)
            .is_some_and(|r| r.target == *target && r.bound_turn.as_ref() == Some(turn))
        {
            return false;
        }
        self.card_instance_tasks
            .coordinator
            .observe_terminal(target, turn, stage, result)
    }

    /// Revocation does not claim to retract messages already owned by the CLI.
    pub fn invalidate_card_instance_delivery(
        &mut self,
        id: &str,
        reason: &str,
    ) -> Option<CardInstanceReceipt> {
        let receipt = self.card_instance_receipt(id)?;
        if let Some(scope) = self.card_instance_tasks.scopes.get(id) {
            scope.authority.store(false, Ordering::Release);
        }
        if matches!(
            receipt.stage,
            CardInstanceStage::Accepted | CardInstanceStage::WaitingInput
        ) {
            let zero = !self.card_instance_tasks.transports.contains_key(id);
            self.fail_card_instance_transport(id, reason, zero);
        }
        self.card_instance_receipt(id)
    }

    pub(crate) fn interrupt_card_instance_tasks(&mut self) {
        let ids: Vec<_> = self
            .card_instance_tasks
            .transports
            .keys()
            .cloned()
            .collect();
        for id in ids {
            self.fail_card_instance_transport(&id, "runtime_stopped_during_input", false);
        }
        self.card_instance_tasks.interrupts.clear();
        self.card_instance_tasks.coordinator.interrupt_unfinished();
    }

    fn validate_card_instance_assignment(
        &self,
        assignment: &CardInstanceAssignment,
        zero: bool,
    ) -> Result<(), CardInstanceDeliveryError> {
        let scope = self
            .card_instance_tasks
            .scopes
            .get(&assignment.assignment_id)
            .ok_or_else(|| delivery_error("route_missing", zero))?;
        if !scope.authority.load(Ordering::Acquire) {
            return Err(delivery_error("authorization_changed", zero));
        }
        if scope.route_token != self.workspace_route_id {
            return Err(delivery_error("route_changed", zero));
        }
        self.runtime
            .validate_workspace_card_target(
                &scope.workspace,
                &scope.area,
                scope.generation,
                &assignment.target,
            )
            .map_err(|e| delivery_error(e.code(), zero))?;
        if zero && self.untracked_card_instance_busy(assignment) {
            return Err(delivery_error("untracked_requires_force", true));
        }
        if let Some(id) = assignment.definition_id {
            if !self
                .agent_definitions
                .definitions()
                .iter()
                .any(|d| d.id == id && d.enabled)
            {
                return Err(delivery_error("role_disabled", zero));
            }
        }
        Ok(())
    }
    fn untracked_card_instance_busy(&self, assignment: &CardInstanceAssignment) -> bool {
        if assignment.tracking != agent::card_instance::CardInstanceTracking::Untracked
            || assignment.force_untracked
        {
            return false;
        }
        matches!(
            self.runtime
                .agent_task_state(assignment.target.card.node_id),
            Some(
                runtime::AgentTaskState::Thinking
                    | runtime::AgentTaskState::RunningTool { .. }
                    | runtime::AgentTaskState::WaitingApproval { .. }
            )
        ) || self
            .card_instance_tasks
            .coordinator
            .has_unfinished_submission(&assignment.target, &assignment.assignment_id)
            || self.agent_task_coordinator.as_ref().is_some_and(|c| {
                c.tasks().iter().any(|t| {
                    t.target_node_id == assignment.target.card.node_id
                        && t.tracking == agent::AgentTrackingMode::Untracked
                        && matches!(t.state, "queued" | "running" | "cancel_requested")
                })
            })
    }

    fn fail_card_instance_transport(&mut self, id: &str, code: &str, zero: bool) {
        if let Some(assignment) = self.card_instance_tasks.coordinator.assignment(id) {
            if zero {
                let _ = self
                    .runtime
                    .release_unwritten_workspace_card_input(&assignment.target, id);
            } else {
                self.runtime
                    .mark_workspace_card_input_unknown(&assignment.target, id);
            }
        }
        self.card_instance_tasks.transports.remove(id);
        let _ = self.card_instance_tasks.coordinator.fail(id, code, zero);
    }
}
fn delivery_error(code: &str, zero_input_proven: bool) -> CardInstanceDeliveryError {
    CardInstanceDeliveryError {
        code: code.into(),
        zero_input_proven,
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/card_instance_delivery_tests.rs"]
mod delivery_tests;
#[cfg(test)]
#[path = "../../../tests/unit/card_instance_evidence_tests.rs"]
mod evidence_tests;
#[cfg(test)]
#[path = "../../../tests/unit/card_instance_input_tests.rs"]
mod input_tests;

#[cfg(test)]
#[path = "../../../tests/support/card_instance_app.rs"]
mod test_support;

#[cfg(test)]
#[path = "../../../tests/unit/workspace_operation_delivery_tests.rs"]
mod workspace_operation_delivery_tests;
