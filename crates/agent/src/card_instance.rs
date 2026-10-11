//! Instance-bound transport and execution facts. Configuration IDs are never addresses.
use norma::protocol::AgentTaskResult;
use norma::workspace_dispatch::WorkspaceCardTarget;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

mod evidence;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardInstanceAssignment {
    pub assignment_id: String,
    pub target: WorkspaceCardTarget,
    pub name_at_acceptance: String,
    pub definition_id: Option<i64>,
    pub text: String,
    pub tracking: CardInstanceTracking,
    /// The workspace service derives this only from explicit user authorization, never a model guess.
    #[serde(default)]
    pub force_untracked: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardInstanceTracking {
    Managed,
    Untracked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardInstanceStage {
    Accepted,
    WaitingInput,
    Submitted,
    Running,
    Succeeded,
    Failed,
    Interrupted,
    Cancelled,
    OutcomeUnknown,
}

impl CardInstanceStage {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Interrupted | Self::Cancelled
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CardInstanceTurn {
    pub run_id: Option<String>,
    pub session_id: String,
    pub turn_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CardInstanceReceipt {
    pub assignment_id: String,
    pub target: WorkspaceCardTarget,
    pub name_at_acceptance: String,
    pub stage: CardInstanceStage,
    pub tracking: CardInstanceTracking,
    pub bound_turn: Option<CardInstanceTurn>,
    pub cancel_requested: bool,
    #[serde(default)]
    pub waiting_reason: Option<String>,
    pub result: Option<AgentTaskResult>,
    pub failure: Option<CardInstanceFailure>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CardInstanceFailure {
    pub code: String,
    /// True only when a system boundary proves that this attempt wrote no input.
    pub retry_safe: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CardInstanceEvent {
    pub receipt: CardInstanceReceipt,
    pub observed_at_ms: u64,
    pub evidence: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CardInstanceError {
    InvalidAssignment,
    Conflict,
    Missing,
    InvalidTransition,
    UntrackedRequiresForce,
}

impl CardInstanceError {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidAssignment => "invalid_assignment",
            Self::Conflict => "assignment_conflict",
            Self::Missing => "assignment_missing",
            Self::InvalidTransition => "invalid_transition",
            Self::UntrackedRequiresForce => "untracked_requires_force",
        }
    }
}
impl std::fmt::Display for CardInstanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for CardInstanceError {}

impl CardInstanceAssignment {
    pub fn validate(&self) -> Result<(), CardInstanceError> {
        if self.target.validate().is_err()
            || self.assignment_id.is_empty()
            || self.assignment_id.len() > 512
            || self.assignment_id.chars().any(char::is_control)
            || self.text.trim().is_empty()
            || self.text.len() > 32 * 1024
            || self
                .text
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err(CardInstanceError::InvalidAssignment);
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct CardInstanceCoordinator {
    assignments: HashMap<String, CardInstanceAssignment>,
    receipts: HashMap<String, CardInstanceReceipt>,
    order: Vec<String>,
    pending: VecDeque<String>,
    events: VecDeque<CardInstanceEvent>,
}

impl CardInstanceCoordinator {
    /// Uses the same conflict and busy policy as acceptance, without mutating the queue or receipts.
    pub fn preflight(&self, assignment: &CardInstanceAssignment) -> Result<(), CardInstanceError> {
        assignment.validate()?;
        if let Some(previous) = self.assignments.get(&assignment.assignment_id) {
            return if previous == assignment {
                Ok(())
            } else {
                Err(CardInstanceError::Conflict)
            };
        }
        if assignment.tracking == CardInstanceTracking::Untracked
            && !assignment.force_untracked
            && self
                .receipts
                .values()
                .any(|r| r.target == assignment.target && !r.stage.is_terminal())
        {
            return Err(CardInstanceError::UntrackedRequiresForce);
        }
        Ok(())
    }

    pub fn accept(
        &mut self,
        assignment: CardInstanceAssignment,
    ) -> Result<CardInstanceReceipt, CardInstanceError> {
        self.preflight(&assignment)?;
        if let Some(previous) = self.assignments.get(&assignment.assignment_id) {
            return if previous == &assignment {
                self.receipt(&assignment.assignment_id)
                    .ok_or(CardInstanceError::Missing)
            } else {
                Err(CardInstanceError::Conflict)
            };
        }
        let receipt = CardInstanceReceipt {
            assignment_id: assignment.assignment_id.clone(),
            target: assignment.target.clone(),
            name_at_acceptance: assignment.name_at_acceptance.clone(),
            stage: CardInstanceStage::Accepted,
            tracking: assignment.tracking,
            bound_turn: None,
            cancel_requested: false,
            waiting_reason: None,
            result: None,
            failure: None,
        };
        self.order.push(assignment.assignment_id.clone());
        self.pending.push_back(assignment.assignment_id.clone());
        self.receipts
            .insert(assignment.assignment_id.clone(), receipt.clone());
        self.assignments
            .insert(assignment.assignment_id.clone(), assignment);
        self.record(&receipt.assignment_id, "accepted");
        Ok(receipt)
    }

    pub fn receipt(&self, id: &str) -> Option<CardInstanceReceipt> {
        self.receipts.get(id).cloned()
    }
    pub fn assignment(&self, id: &str) -> Option<&CardInstanceAssignment> {
        self.assignments.get(id)
    }
    pub fn pending_assignments(&self) -> Vec<CardInstanceAssignment> {
        self.pending
            .iter()
            .filter_map(|id| {
                let receipt = self.receipts.get(id)?;
                matches!(
                    receipt.stage,
                    CardInstanceStage::Accepted | CardInstanceStage::WaitingInput
                )
                .then(|| self.assignments[id].clone())
            })
            .collect()
    }
    pub fn has_unfinished_submission(&self, target: &WorkspaceCardTarget, exclude: &str) -> bool {
        self.receipts.values().any(|r| {
            r.assignment_id != exclude
                && r.target == *target
                && matches!(
                    r.stage,
                    CardInstanceStage::Submitted
                        | CardInstanceStage::Running
                        | CardInstanceStage::OutcomeUnknown
                )
        })
    }
    pub fn take_events(&mut self) -> Vec<CardInstanceEvent> {
        self.events.drain(..).collect()
    }

    pub fn waiting(&mut self, id: &str, reason: &str) -> Result<(), CardInstanceError> {
        let receipt = self
            .receipts
            .get_mut(id)
            .ok_or(CardInstanceError::Missing)?;
        if !matches!(
            receipt.stage,
            CardInstanceStage::Accepted | CardInstanceStage::WaitingInput
        ) {
            return Err(CardInstanceError::InvalidTransition);
        }
        if receipt.stage != CardInstanceStage::WaitingInput
            || receipt.waiting_reason.as_deref() != Some(reason)
        {
            receipt.stage = CardInstanceStage::WaitingInput;
            receipt.waiting_reason = Some(reason.into());
            self.record(id, reason);
        }
        Ok(())
    }
    pub fn submitted(&mut self, id: &str) -> Result<(), CardInstanceError> {
        let receipt = self
            .receipts
            .get_mut(id)
            .ok_or(CardInstanceError::Missing)?;
        if !matches!(
            receipt.stage,
            CardInstanceStage::Accepted | CardInstanceStage::WaitingInput
        ) {
            return Err(CardInstanceError::InvalidTransition);
        }
        receipt.stage = CardInstanceStage::Submitted;
        receipt.waiting_reason = None;
        self.pending.retain(|key| key != id);
        self.record(id, "enter_confirmed");
        Ok(())
    }
    pub fn fail(
        &mut self,
        id: &str,
        code: &str,
        zero_input_proven: bool,
    ) -> Result<(), CardInstanceError> {
        let receipt = self
            .receipts
            .get_mut(id)
            .ok_or(CardInstanceError::Missing)?;
        if receipt.stage.is_terminal() {
            return Err(CardInstanceError::InvalidTransition);
        }
        let zero_input_proven = zero_input_proven
            && matches!(
                receipt.stage,
                CardInstanceStage::Accepted | CardInstanceStage::WaitingInput
            );
        receipt.stage = if zero_input_proven {
            CardInstanceStage::Failed
        } else {
            CardInstanceStage::OutcomeUnknown
        };
        receipt.waiting_reason = None;
        receipt.failure = Some(CardInstanceFailure {
            code: code.into(),
            retry_safe: zero_input_proven,
        });
        self.pending.retain(|key| key != id);
        self.record(id, code);
        Ok(())
    }
    /// The caller proves no draft/Enter side effect before invoking this transition.
    pub fn cancel_unsubmitted(&mut self, id: &str) -> Result<(), CardInstanceError> {
        let receipt = self
            .receipts
            .get_mut(id)
            .ok_or(CardInstanceError::Missing)?;
        if !matches!(
            receipt.stage,
            CardInstanceStage::Accepted | CardInstanceStage::WaitingInput
        ) {
            return Err(CardInstanceError::InvalidTransition);
        }
        receipt.stage = CardInstanceStage::Cancelled;
        receipt.waiting_reason = None;
        self.pending.retain(|key| key != id);
        self.record(id, "cancelled_before_input");
        Ok(())
    }
    pub fn request_cancel(&mut self, id: &str) -> Result<(), CardInstanceError> {
        let receipt = self
            .receipts
            .get_mut(id)
            .ok_or(CardInstanceError::Missing)?;
        if receipt.stage != CardInstanceStage::Running || receipt.bound_turn.is_none() {
            return Err(CardInstanceError::InvalidTransition);
        }
        if !receipt.cancel_requested {
            receipt.cancel_requested = true;
            self.record(id, "interrupt_requested");
        }
        Ok(())
    }
    pub fn interrupt_unfinished(&mut self) {
        self.pending.clear();
        let ids = self.order.clone();
        for id in ids {
            if self.receipts.get(&id).is_some_and(|r| {
                !r.stage.is_terminal() && r.stage != CardInstanceStage::OutcomeUnknown
            }) {
                let receipt = self.receipts.get_mut(&id).expect("receipt exists");
                let zero = matches!(
                    receipt.stage,
                    CardInstanceStage::Accepted | CardInstanceStage::WaitingInput
                );
                receipt.stage = if zero {
                    CardInstanceStage::Interrupted
                } else {
                    CardInstanceStage::OutcomeUnknown
                };
                receipt.waiting_reason = None;
                receipt.failure = Some(CardInstanceFailure {
                    code: "runtime_stopped_no_replay".into(),
                    retry_safe: zero,
                });
                self.record(&id, "runtime_stopped_no_replay");
            }
        }
    }
    fn record(&mut self, id: &str, evidence: &str) {
        let observed_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64)
            .unwrap_or(0);
        if let Some(receipt) = self.receipts.get(id) {
            self.events.push_back(CardInstanceEvent {
                receipt: receipt.clone(),
                observed_at_ms,
                evidence: evidence.into(),
            });
        }
    }
}
