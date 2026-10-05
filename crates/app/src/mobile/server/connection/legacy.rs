//! Legacy thread compatibility is strictly read-only and shared by LAN and relay.
use super::{failure, thread_delivery, Action, Request, Shared};
use serde_json::Value;
use std::sync::Arc;

pub(super) fn reject_mutation(text: &str) -> Result<(), Value> {
    let Ok(envelope) = serde_json::from_str::<Value>(text) else {
        return Ok(());
    };
    if !matches!(
        envelope["action"]["type"].as_str(),
        Some(
            "thread_message"
                | "thread_task"
                | "thread_task_followup"
                | "thread_task_accept"
                | "thread_task_terminal_control"
        )
    ) {
        return Ok(());
    }
    let id = envelope["request_id"].as_str().unwrap_or_default();
    if !crate::mobile::command::valid_id(id) {
        return Err(failure("", "invalid_request"));
    }
    if envelope["version"].as_u64() != Some(1) {
        return Err(failure(id, "unsupported_version"));
    }
    Err(failure(id, "legacy_thread_read_only"))
}

pub(super) fn credentials(action: &Action) -> Option<(&str, &str)> {
    match action {
        Action::ThreadTaskFollowupReceipt {
            device_id, token, ..
        }
        | Action::ThreadTaskTerminal {
            device_id, token, ..
        }
        | Action::ThreadTaskAcceptance {
            device_id, token, ..
        }
        | Action::ThreadTaskReceipt {
            device_id, token, ..
        }
        | Action::ThreadTaskReview {
            device_id, token, ..
        }
        | Action::ThreadReceipt {
            device_id, token, ..
        }
        | Action::ThreadEvents {
            device_id, token, ..
        } => Some((device_id, token)),
        _ => None,
    }
}

pub(super) fn read(
    shared: &Arc<Shared>,
    request: &Request,
    device: &str,
    token: &str,
) -> Result<Value, &'static str> {
    match &request.action {
        Action::ThreadTaskFollowupReceipt { followup, .. } => {
            crate::mobile::thread_followup::handle(
                shared,
                &request.machine_id,
                device,
                token,
                followup,
            )
        }
        Action::ThreadTaskTerminal { request: scope, .. } => {
            crate::mobile::thread_task_terminal::read(
                shared,
                &request.machine_id,
                device,
                token,
                scope,
            )
        }
        Action::ThreadTaskAcceptance { decision, .. } => {
            crate::mobile::thread_task_acceptance::handle(
                shared,
                &request.machine_id,
                device,
                token,
                decision,
            )
        }
        Action::ThreadTaskReview { request: scope, .. } => crate::mobile::thread_task_review::read(
            shared,
            &request.machine_id,
            device,
            token,
            scope,
        ),
        Action::ThreadTaskReceipt {
            collaboration_workspace_id,
            thread_id,
            operation_id,
            ..
        } => crate::mobile::thread_task::receipt(
            shared,
            &request.machine_id,
            device,
            token,
            collaboration_workspace_id,
            thread_id,
            operation_id,
        ),
        Action::ThreadReceipt { .. } | Action::ThreadEvents { .. } => {
            thread_delivery::handle(shared, request, device, token)
        }
        _ => Err("invalid_request"),
    }
}
