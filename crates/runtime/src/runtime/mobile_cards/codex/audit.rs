use super::*;
use norma::codex::CodexApprovalState;
use std::{collections::VecDeque, sync::mpsc};
use storage::{ApprovalAuditEntry, ApprovalAuditStore, BackgroundApprovalAudit, StorageResult};

type Chosen = (String, ApprovalDecision);
type Receipt = (Vec<Chosen>, mpsc::Receiver<StorageResult<()>>);

pub(super) struct CodexAudit {
    node_id: NodeId,
    writer: BackgroundApprovalAudit,
    receipts: VecDeque<Receipt>,
}

impl CodexAudit {
    pub(super) fn new(node_id: NodeId, store: ApprovalAuditStore) -> Result<Self, String> {
        Ok(Self {
            node_id,
            writer: BackgroundApprovalAudit::new(store).map_err(|e| e.to_string())?,
            receipts: VecDeque::new(),
        })
    }

    pub(super) fn record(&mut self, changes: Vec<CodexApprovalChange>) -> Result<(), String> {
        if changes.is_empty() {
            return Ok(());
        }
        // Match the writer's 8-batch bound; a stalled disk cannot grow foreground memory.
        if self.receipts.len() >= 8 {
            return Err("approval audit receipts full".into());
        }
        let mut chosen = Vec::new();
        let entries = changes
            .into_iter()
            .map(|change| {
                if let CodexApprovalState::Chosen(decision) = change.state {
                    chosen.push((change.approval.id.clone(), decision));
                }
                let mut payload = serde_json::to_value(&change).map_err(|e| e.to_string())?;
                if let CodexApprovalState::Chosen(decision)
                | CodexApprovalState::Submitted(decision) = change.state
                {
                    payload["provider_response"] = change.approval.response(decision)?;
                }
                Ok(ApprovalAuditEntry {
                    run_id: change.approval.run_id.clone(),
                    sequence: change.sequence,
                    node_id: self.node_id,
                    provider: "codex".into(),
                    request_id: change.approval.id.clone(),
                    payload,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let receipt = self.writer.append(entries).map_err(|e| e.to_string())?;
        self.receipts.push_back((chosen, receipt));
        Ok(())
    }

    pub(super) fn record_turn(
        &mut self,
        run: &str,
        session: &str,
        turn: &str,
        state: storage::MobileTurnState,
    ) -> Result<(), String> {
        if self.receipts.len() >= 8 {
            return Err("turn outcome receipts full".into());
        }
        let outcome = storage::MobileTurnOutcome {
            key: storage::MobileTurnKey {
                node_id: self.node_id,
                profile: "Codex".into(),
                run_id: run.into(),
                session_id: session.into(),
                turn_id: turn.into(),
            },
            state,
        };
        let receipt = self
            .writer
            .record_turn(outcome)
            .map_err(|e| e.to_string())?;
        self.receipts.push_back((Vec::new(), receipt));
        Ok(())
    }

    pub(super) fn committed(&mut self) -> Result<Vec<Chosen>, String> {
        let mut choices = Vec::new();
        while let Some((_, receipt)) = self.receipts.front() {
            match receipt.try_recv() {
                Ok(Ok(())) => choices.extend(self.receipts.pop_front().expect("front exists").0),
                Ok(Err(error)) => return Err(format!("approval audit commit failed: {error}")),
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("approval audit writer disconnected".into())
                }
                Err(mpsc::TryRecvError::Empty) => break,
            }
        }
        Ok(choices)
    }
}

impl CodexHost {
    pub(super) fn drive_audit(&mut self) -> Result<(), String> {
        // Provider notifications were drained first. A durable choice never resurrects a stale request.
        for (id, decision) in self.audit.committed()? {
            if !self.control.is_cancelled()
                && self.connection.is_connected()
                && self.approvals.chosen_decision(&id, Instant::now())? == Some(decision)
            {
                self.approvals
                    .submit(&id, decision, Instant::now(), |id, result| {
                        self.connection.respond(id, result)
                    })?;
            }
        }
        self.audit.record(self.approvals.take_changes())
    }
}
