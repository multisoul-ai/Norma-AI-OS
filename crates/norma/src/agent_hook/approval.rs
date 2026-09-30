//! One permission Hook invocation; never deduplicate by tool name or command text.
use super::AgentHookEvent;
use crate::protocol::AgentProfile;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, Read};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const ENV_CONTROL_RUN_ID: &str = "NORMA_CONTROL_RUN_ID";
pub const APPROVAL_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HookApprovalRequest {
    pub run_id: String,
    pub event: AgentHookEvent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Allow,
    Deny,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Chosen(ApprovalDecision),
    Decided(ApprovalDecision),
    Responded(ApprovalDecision),
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct HookApproval(Arc<Invocation>);

#[derive(Debug)]
struct Invocation {
    id: String,
    request: HookApprovalRequest,
    expires_at: Instant,
    status: Mutex<ApprovalStatus>,
}

impl HookApproval {
    pub fn new(request: HookApprovalRequest, now: Instant) -> io::Result<Self> {
        let event = &request.event;
        if request.run_id.is_empty()
            || request.run_id.len() > 128
            || event.profile != AgentProfile::Claude
            || event.provider_event != "PermissionRequest"
            || event.session_id.as_deref().is_none_or(str::is_empty)
            || event.tool_name.as_deref().is_none_or(str::is_empty)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid permission invocation",
            ));
        }
        Ok(Self(Arc::new(Invocation {
            id: fresh_control_id()?,
            request,
            expires_at: now + APPROVAL_TIMEOUT,
            status: Mutex::new(ApprovalStatus::Pending),
        })))
    }

    pub fn id(&self) -> &str {
        &self.0.id
    }
    pub fn request(&self) -> &HookApprovalRequest {
        &self.0.request
    }

    pub fn status(&self, now: Instant) -> ApprovalStatus {
        let mut status = self.0.status.lock().expect("permission state poisoned");
        self.expire(&mut status, now);
        *status
    }

    pub fn decide(&self, decision: ApprovalDecision, now: Instant) -> bool {
        self.transition(
            ApprovalStatus::Pending,
            ApprovalStatus::Decided(decision),
            now,
        )
    }

    /// Reserve a mobile choice without making it visible to the Hook transport.
    pub fn choose(&self, decision: ApprovalDecision, now: Instant) -> bool {
        self.transition(
            ApprovalStatus::Pending,
            ApprovalStatus::Chosen(decision),
            now,
        )
    }

    /// Call only after durable audit commit; cancellation/timeout wins over a late receipt.
    pub fn submit_chosen(&self, decision: ApprovalDecision, now: Instant) -> bool {
        self.transition(
            ApprovalStatus::Chosen(decision),
            ApprovalStatus::Decided(decision),
            now,
        )
    }

    fn transition(&self, from: ApprovalStatus, to: ApprovalStatus, now: Instant) -> bool {
        let mut status = self.0.status.lock().expect("permission state poisoned");
        self.expire(&mut status, now);
        if *status != from {
            return false;
        }
        *status = to;
        true
    }

    /// A disconnected Hook or ended run can revoke even an unsent decision.
    pub fn cancel(&self) {
        let mut status = self.0.status.lock().expect("permission state poisoned");
        if !matches!(*status, ApprovalStatus::Responded(_)) {
            *status = ApprovalStatus::Cancelled;
        }
    }

    /// Consumed by the transport, not proof that the Provider executed the tool.
    pub fn take_response(&self, now: Instant) -> Option<Value> {
        let mut status = self.0.status.lock().expect("permission state poisoned");
        self.expire(&mut status, now);
        let ApprovalStatus::Decided(decision) = *status else {
            return None;
        };
        *status = ApprovalStatus::Responded(decision);
        Some(claude_permission_output(decision))
    }

    fn expire(&self, status: &mut ApprovalStatus, now: Instant) {
        if now >= self.0.expires_at
            && matches!(
                status,
                ApprovalStatus::Pending | ApprovalStatus::Chosen(_) | ApprovalStatus::Decided(_)
            )
        {
            *status = ApprovalStatus::Cancelled;
        }
    }
}

pub fn claude_permission_output(decision: ApprovalDecision) -> Value {
    let decision = match decision {
        ApprovalDecision::Allow => json!({"behavior": "allow"}),
        ApprovalDecision::Deny => {
            json!({"behavior": "deny", "message": "User declined this request in Norma."})
        }
    };
    json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": decision}})
}

/// Secure per-run / per-invocation UUID. Failure must never fall back to a clock.
/// Call during background preparation, not on a UI hot path.
pub fn fresh_control_id() -> io::Result<String> {
    let mut bytes = [0_u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = hex::encode(bytes);
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}
