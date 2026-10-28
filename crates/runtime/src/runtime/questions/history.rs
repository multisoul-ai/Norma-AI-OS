//! Retain recent details and compact replay identities only after the host owns the receipt.
use super::*;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

type Identity = [u8; 32];

#[derive(Default)]
pub(super) struct QuestionHistory {
    consumed: HashMap<NodeId, HashSet<Identity>>,
    revisions: HashMap<NodeId, HashMap<Identity, u64>>,
}

fn identity(target: &QuestionRef, connection: bool) -> Identity {
    let value = serde_json::json!([
        target.workspace_id,
        target.project_id,
        target.node_id,
        target.provider,
        (connection || target.provider != norma::protocol::AgentProfile::OpenCode)
            .then_some(&target.run_id),
        target.session_id,
        target.turn_id,
        target.request_id,
        connection.then_some(&target.connection_id),
    ]);
    Sha256::digest(value.to_string().as_bytes()).into()
}

impl QuestionHistory {
    pub(super) fn check(&self, target: &QuestionRef) -> Result<(), &'static str> {
        if self
            .consumed
            .get(&target.node_id)
            .is_some_and(|ids| ids.contains(&identity(target, false)))
        {
            return Err("question_already_answered");
        }
        if self
            .revisions
            .get(&target.node_id)
            .and_then(|ids| ids.get(&identity(target, true)))
            .is_some_and(|revision| *revision >= target.revision)
        {
            return Err("question_revision_stale");
        }
        Ok(())
    }

    fn remember(&mut self, snapshot: &QuestionSnapshot) {
        let target = &snapshot.question.target;
        if matches!(
            snapshot.state,
            QuestionState::Answered
                | QuestionState::AnsweredElsewhere
                | QuestionState::ConsumedUnattributed
        ) {
            self.consumed
                .entry(target.node_id)
                .or_default()
                .insert(identity(target, false));
        } else {
            let revision = self
                .revisions
                .entry(target.node_id)
                .or_default()
                .entry(identity(target, true))
                .or_default();
            *revision = (*revision).max(target.revision);
        }
    }

    pub(super) fn remove_node(&mut self, node: NodeId) {
        self.consumed.remove(&node);
        self.revisions.remove(&node);
    }
}

impl Questions {
    pub(super) fn compact_history(&mut self) {
        let removable =
            |entry: &Entry| entry.receipt_pending.is_none() && !entry.snapshot.state.blocks_input();
        let mut remaining = self
            .entries
            .iter()
            .filter(|entry| removable(entry))
            .count()
            .saturating_sub(64);
        self.entries.retain(|entry| {
            if remaining == 0 || !removable(entry) {
                return true;
            }
            self.history.remember(&entry.snapshot);
            remaining -= 1;
            false
        });
    }
}

impl Runtime {
    /// The host copied a final outcome, or reserve ended without a lease. Never performs IO.
    pub fn acknowledge_question_receipt(&mut self, submission: &QuestionSubmission) {
        if let Some(entry) = self.questions.entries.iter_mut().find(|entry| {
            entry.snapshot.question.target == *submission.target()
                && entry.receipt_pending.as_deref() == Some(submission.delivery_id())
        }) {
            entry.receipt_pending = None;
        }
    }
}
