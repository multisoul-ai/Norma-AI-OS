use crate::agent_hook::ApprovalDecision;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Provider Hook IDs can contain private configuration paths. Keep a stable, separate namespace
/// for informational mobile entries without transmitting the original ID or its embedded path.
pub fn redacted_external_hook_id(raw: &str) -> String {
    format!("external:{}", hex::encode(Sha256::digest(raw.as_bytes())))
}

/// Each invocation is immutable. A later request, even with identical tool input, has a new ID.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ApprovalTarget {
    pub node_id: u64,
    pub profile: String,
    pub run_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub request_id: String,
}

impl ApprovalTarget {
    /// Interpret a project audit entry; transport submission never proves tool execution or a winner.
    pub fn delivery_state(
        &self,
        payload: &Value,
        decision: ApprovalDecision,
    ) -> Result<&'static str, &'static str> {
        if !self.valid() {
            return Err("invalid_request");
        }
        let (identity, submitted) = if self.profile == "Codex" {
            let request = &payload["approval"];
            (
                request["id"] == self.request_id
                    && request["run_id"] == self.run_id
                    && request["thread_id"] == self.session_id
                    && request["turn_id"] == self.turn_id,
                "submitted",
            )
        } else {
            let request = &payload["request"];
            let event = &request["event"];
            (
                payload["id"] == self.request_id
                    && request["run_id"] == self.run_id
                    && event["node_id"].as_u64() == Some(self.node_id)
                    && event["session_id"] == self.session_id
                    && event["turn_id"] == self.turn_id,
                "responded",
            )
        };
        if !identity {
            return Err("approval_changed");
        }
        let state = &payload["state"];
        match state.as_str() {
            Some("pending") => return Ok("waiting"),
            Some("cancelled" | "resolved" | "expired") => return Ok("request_closed"),
            Some(_) => return Err("approval_unavailable"),
            None => {}
        }
        let fields = state
            .as_object()
            .filter(|fields| fields.len() == 1)
            .ok_or("approval_unavailable")?;
        let (stage, chosen) = fields.iter().next().ok_or("approval_unavailable")?;
        if chosen != &serde_json::to_value(decision).map_err(|_| "approval_unavailable")? {
            return Err("decision_conflict");
        }
        if stage == submitted {
            return Ok("submitted");
        }
        if stage == "chosen" || (self.profile == "Claude" && stage == "decided") {
            return Ok("waiting");
        }
        Err("approval_unavailable")
    }

    pub fn valid(&self) -> bool {
        self.node_id > 0
            && matches!(self.profile.as_str(), "Codex" | "Claude")
            && [&self.run_id, &self.session_id, &self.turn_id].into_iter().all(|s|
                !s.is_empty() && s.len() <= 128
                    && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
            // Codex uses run_id:invocation; Claude uses a fresh UUID.
            && !self.request_id.is_empty() && self.request_id.len() <= 256
            && self.request_id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':'))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ApprovalSnapshot {
    pub target: ApprovalTarget,
    pub card_title: String,
    pub tool: String,
    pub details: Value,
    /// Only single-invocation choices offered by the Provider; no session-wide permission bypass.
    pub decisions: Vec<ApprovalDecision>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ApprovalListRequest {
    pub workspace_id: String,
    pub route_id: String,
}

impl ApprovalListRequest {
    pub fn valid(&self) -> bool {
        [&self.workspace_id, &self.route_id].into_iter().all(|s| {
            !s.is_empty()
                && s.len() <= 128
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
    }
}
