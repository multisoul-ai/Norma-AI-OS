//! Only the attached native thread can start or finish a card's turn.
use super::CodexAttachment;
use serde_json::Value;
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct CodexTurnEvent {
    pub attachment: CodexAttachment,
    pub turn_id: String,
    /// None means started; only explicit terminal Provider statuses finish a turn.
    pub status: Option<String>,
    pub message: Option<String>,
}

#[derive(Default)]
pub(super) struct NativeTurns {
    attachment: Option<CodexAttachment>,
    active: Option<String>,
    retired: VecDeque<String>,
    last_message: Option<String>,
}

impl NativeTurns {
    pub(super) fn attach(&mut self, attachment: CodexAttachment) {
        *self = Self {
            attachment: Some(attachment),
            ..Self::default()
        };
    }

    pub(super) fn observe(&mut self, event: &Value) -> Option<CodexTurnEvent> {
        let attachment = self.attachment.as_ref()?;
        let params = &event["params"];
        if params["threadId"] != attachment.thread_id() {
            return None;
        }
        let method = event["method"].as_str()?;
        if method == "item/completed"
            && params["turnId"].as_str() == self.active.as_deref()
            && params["item"]["type"] == "agentMessage"
            && params["item"]["phase"] != "commentary"
        {
            self.last_message = params["item"]["text"].as_str().map(str::to_owned);
            return None;
        }
        if !matches!(method, "turn/started" | "turn/completed") {
            return None;
        }
        let turn = params["turn"]["id"]
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 256)?;
        let status = if method == "turn/started" {
            if self.active.as_deref() == Some(turn) || self.retired.iter().any(|id| id == turn) {
                return None;
            }
            self.active = Some(turn.into());
            self.last_message = None;
            None
        } else {
            if self.active.as_deref() != Some(turn) {
                return None;
            }
            let status = params["turn"]["status"].as_str()?;
            if !matches!(status, "completed" | "failed" | "interrupted") {
                return None;
            }
            self.active = None;
            // Same 128-turn replay window as the native input guard.
            if self.retired.len() == 128 {
                self.retired.pop_front();
            }
            self.retired.push_back(turn.into());
            Some(status.into())
        };
        let message = match status.as_deref() {
            Some("failed") => params["turn"]["error"]["message"]
                .as_str()
                .map(str::to_owned),
            Some("completed") => self.last_message.clone(),
            _ => None,
        };
        Some(CodexTurnEvent {
            attachment: attachment.clone(),
            turn_id: turn.into(),
            status,
            message,
        })
    }
}
