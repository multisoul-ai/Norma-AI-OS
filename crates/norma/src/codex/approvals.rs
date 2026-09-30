use crate::agent_hook::{ApprovalDecision, APPROVAL_TIMEOUT};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Instant;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CodexApproval {
    /// Local invocation identity; Provider IDs may be reused after resolution.
    pub id: String,
    pub run_id: String,
    pub provider_id: Value,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub method: String,
    pub params: Value,
}

impl CodexApproval {
    /// The rejection can end the turn when the Provider offers cancel instead of decline.
    pub fn response(&self, decision: ApprovalDecision) -> Result<Value, String> {
        let choices: &[&str] = match decision {
            ApprovalDecision::Allow => &["accept"],
            ApprovalDecision::Deny => &["decline", "cancel"],
        };
        let offered = self
            .params
            .get("availableDecisions")
            .filter(|value| !value.is_null());
        let choice = choices
            .iter()
            .find(|choice| {
                offered.is_none_or(|list| {
                    list.as_array()
                        .is_some_and(|values| values.iter().any(|value| value == **choice))
                })
            })
            .ok_or("Provider did not offer this single-request decision")?;
        Ok(json!({"decision":choice}))
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexApprovalState {
    Pending,
    /// Local choice reserved for persistence; no Provider response has been sent.
    Chosen(ApprovalDecision),
    /// Transport accepted a local attempt. It does not prove who won or that a tool ran.
    Submitted(ApprovalDecision),
    /// Provider cleared the request; the notification does not disclose the winner's choice.
    Resolved,
    Expired,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CodexApprovalChange {
    pub sequence: u64,
    pub approval: CodexApproval,
    pub state: CodexApprovalState,
}

struct Active {
    approval: CodexApproval,
    deadline: Instant,
    state: CodexApprovalState,
}

pub struct CodexApprovals {
    run_id: String,
    thread_id: String,
    turn_id: Option<String>,
    next_invocation: u64,
    next_change: u64,
    active: Vec<Active>,
    changes: Vec<CodexApprovalChange>,
    failed: bool,
}

impl CodexApprovals {
    pub fn new(run_id: String, thread_id: String, turn_id: Option<String>) -> Self {
        Self {
            run_id,
            thread_id,
            turn_id,
            next_invocation: 0,
            next_change: 0,
            active: Vec::new(),
            changes: Vec::new(),
            failed: false,
        }
    }

    pub fn turn_id(&self) -> Option<&str> {
        self.turn_id.as_deref()
    }

    pub fn observe(&mut self, event: &Value, now: Instant) -> Result<(), String> {
        self.expire(now)?;
        let params = &event["params"];
        if params["threadId"] != self.thread_id {
            return Ok(());
        }
        match event["method"].as_str() {
            Some("turn/started") => {
                let turn = params["turn"]["id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .ok_or("missing Codex turn ID")?;
                if self.turn_id.as_deref() != Some(turn) {
                    self.cancel_all()?;
                    self.turn_id = Some(turn.into());
                }
            }
            Some("turn/completed") if params["turn"]["id"].as_str() == self.turn_id.as_deref() => {
                self.cancel_all()?;
                self.turn_id = None;
            }
            Some("serverRequest/resolved") => {
                if let Some(index) = self
                    .active
                    .iter()
                    .position(|active| active.approval.provider_id == params["requestId"])
                {
                    self.room_for(1)?;
                    let active = self.active.remove(index);
                    self.record(active.approval, CodexApprovalState::Resolved);
                }
            }
            Some("item/commandExecution/requestApproval" | "item/fileChange/requestApproval") => {
                if self.turn_id.is_none() || params["turnId"].as_str() != self.turn_id.as_deref() {
                    return Ok(());
                }
                let id = &event["id"];
                if !(id.is_i64() || id.as_str().is_some_and(|id| !id.is_empty())) {
                    return Err("invalid Codex approval ID".into());
                }
                let item_id = params["itemId"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .ok_or("missing Codex item ID")?;
                if self
                    .active
                    .iter()
                    .any(|active| &active.approval.provider_id == id)
                {
                    return Ok(());
                }
                // 64 active requests * 64 KiB payload = at most 4 MiB active approval context.
                if self.active.len() >= 64 || params.to_string().len() > 64 * 1024 {
                    self.failed = true;
                    return Err("Codex approval capacity exceeded".into());
                }
                self.room_for(1)?;
                self.next_invocation = self
                    .next_invocation
                    .checked_add(1)
                    .ok_or("Codex approval identity exhausted")?;
                let approval = CodexApproval {
                    id: format!("{}:{}", self.run_id, self.next_invocation),
                    run_id: self.run_id.clone(),
                    provider_id: id.clone(),
                    thread_id: self.thread_id.clone(),
                    turn_id: self.turn_id.clone().expect("validated turn"),
                    item_id: item_id.into(),
                    method: event["method"].as_str().expect("matched method").into(),
                    params: params.clone(),
                };
                self.record(approval.clone(), CodexApprovalState::Pending);
                self.active.push(Active {
                    approval,
                    deadline: now + APPROVAL_TIMEOUT,
                    state: CodexApprovalState::Pending,
                });
            }
            _ => {}
        }
        Ok(())
    }

    pub fn pending(&mut self, now: Instant) -> Result<Vec<CodexApproval>, String> {
        self.expire(now)?;
        Ok(self
            .active
            .iter()
            .filter(|active| active.state == CodexApprovalState::Pending)
            .map(|active| active.approval.clone())
            .collect())
    }

    pub fn choose(
        &mut self,
        id: &str,
        decision: ApprovalDecision,
        now: Instant,
    ) -> Result<(), String> {
        self.expire(now)?;
        self.room_for(1)?;
        let index = self
            .active
            .iter()
            .position(|active| {
                active.approval.id == id && active.state == CodexApprovalState::Pending
            })
            .ok_or("Codex approval is no longer pending")?;
        let approval = self.active[index].approval.clone();
        approval.response(decision)?;
        self.active[index].state = CodexApprovalState::Chosen(decision);
        self.record(approval, CodexApprovalState::Chosen(decision));
        Ok(())
    }

    pub fn chosen_decision(
        &mut self,
        id: &str,
        now: Instant,
    ) -> Result<Option<ApprovalDecision>, String> {
        self.expire(now)?;
        Ok(self
            .active
            .iter()
            .find(|entry| entry.approval.id == id)
            .and_then(|entry| match entry.state {
                CodexApprovalState::Chosen(decision) => Some(decision),
                _ => None,
            }))
    }

    /// Call only after the Chosen audit entry committed; this method revalidates live state.
    pub fn submit(
        &mut self,
        id: &str,
        decision: ApprovalDecision,
        now: Instant,
        send: impl FnOnce(Value, Value) -> Result<(), String>,
    ) -> Result<(), String> {
        self.expire(now)?;
        self.room_for(1)?;
        let index = self
            .active
            .iter()
            .position(|entry| {
                entry.approval.id == id && entry.state == CodexApprovalState::Chosen(decision)
            })
            .ok_or("Codex choice is no longer live")?;
        let approval = self.active[index].approval.clone();
        send(approval.provider_id.clone(), approval.response(decision)?)?;
        self.active[index].state = CodexApprovalState::Submitted(decision);
        self.record(approval, CodexApprovalState::Submitted(decision));
        Ok(())
    }

    pub fn cancel_all(&mut self) -> Result<(), String> {
        self.room_for(self.active.len())?;
        let active = std::mem::take(&mut self.active);
        for entry in active {
            self.record(entry.approval, CodexApprovalState::Cancelled);
        }
        Ok(())
    }

    pub fn take_changes(&mut self) -> Vec<CodexApprovalChange> {
        std::mem::take(&mut self.changes)
    }

    fn expire(&mut self, now: Instant) -> Result<(), String> {
        let count = self
            .active
            .iter()
            .filter(|entry| {
                matches!(
                    entry.state,
                    CodexApprovalState::Pending | CodexApprovalState::Chosen(_)
                ) && now >= entry.deadline
            })
            .count();
        self.room_for(count)?;
        let mut index = 0;
        while index < self.active.len() {
            if matches!(
                self.active[index].state,
                CodexApprovalState::Pending | CodexApprovalState::Chosen(_)
            ) && now >= self.active[index].deadline
            {
                self.active[index].state = CodexApprovalState::Expired;
                self.record(
                    self.active[index].approval.clone(),
                    CodexApprovalState::Expired,
                );
            }
            index += 1;
        }
        Ok(())
    }

    fn room_for(&mut self, count: usize) -> Result<(), String> {
        // 256 changes * 64 KiB = 16 MiB ceiling until the audit consumer drains them.
        if self.failed
            || self.changes.len() + count > 256
            || self.next_change.checked_add(count as u64).is_none()
        {
            self.failed = true;
            Err("Codex approval audit consumer unavailable".into())
        } else {
            Ok(())
        }
    }

    fn record(&mut self, approval: CodexApproval, state: CodexApprovalState) {
        self.next_change += 1; // Every caller reserves capacity with room_for first.
        self.changes.push(CodexApprovalChange {
            sequence: self.next_change,
            approval,
            state,
        });
    }
}
