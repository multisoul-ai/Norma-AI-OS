use super::{decode, failure, success, Action, Shared};
use anyhow::{ensure, Result};
use serde_json::json;
use std::sync::Arc;

/// Only explicitly supported RPCs cross this transport. Pairing and streaming are local-only.
pub(in crate::mobile::server) fn handle(
    text: &str,
    shared: &Arc<Shared>,
    authenticated: &mut Option<String>,
    terminal: &mut crate::mobile::thread_terminal_control::Connection,
) -> Result<Vec<u8>> {
    let request = decode(text).map_err(|_| anyhow::anyhow!("invalid relay request"))?;
    let (device, token) = match &request.action {
        Action::Authenticate { device_id, token }
        | Action::ThreadTaskFollowup {
            device_id, token, ..
        }
        | Action::ThreadTaskFollowupReceipt {
            device_id, token, ..
        }
        | Action::ThreadTaskTerminal {
            device_id, token, ..
        }
        | Action::ThreadTaskTerminalControl {
            device_id, token, ..
        }
        | Action::ThreadTaskAccept {
            device_id, token, ..
        }
        | Action::ThreadTaskAcceptance {
            device_id, token, ..
        }
        | Action::ThreadTask {
            device_id, token, ..
        }
        | Action::ThreadTaskReceipt {
            device_id, token, ..
        }
        | Action::ThreadTaskReview {
            device_id, token, ..
        }
        | Action::ThreadMessage {
            device_id, token, ..
        }
        | Action::ThreadReceipt {
            device_id, token, ..
        }
        | Action::ThreadEvents {
            device_id, token, ..
        }
        | Action::Snapshot {
            device_id, token, ..
        } => (device_id, token),
        _ => anyhow::bail!("unsupported relay action"),
    };
    ensure!(
        authenticated.as_ref().is_none_or(|id| id == device),
        "relay device changed"
    );
    ensure!(
        authenticated.is_some() || matches!(request.action, Action::Authenticate { .. }),
        "relay authentication required"
    );
    let grant = shared
        .access()?
        .authenticate(&request.machine_id, device, token)?
        .ok_or_else(|| anyhow::anyhow!("unauthorized relay device"))?;
    let reply = match &request.action {
        Action::Authenticate { .. } => {
            *authenticated = Some(device.clone());
            let mut capabilities = vec!["thread_delivery_v1"];
            if shared.commands.is_some() {
                capabilities.push("thread_tasks_v1");
                capabilities.push("thread_task_results_v1");
                capabilities.push("thread_task_review_v1");
                capabilities.push("thread_task_acceptance_v1");
                capabilities.push("thread_task_revision_v1");
                if shared.terminals.is_some() {
                    if shared.snapshots.is_some() {
                        capabilities.push("thread_task_followup_v1");
                    }
                    capabilities.push("thread_task_terminal_v1");
                    capabilities.push("thread_task_terminal_control_v1");
                }
            }
            // Receipt support does not advertise the still-separate execution capability.
            success(
                &request.request_id,
                json!({"status":"authorized", "grant":grant, "capabilities":capabilities}),
            )
        }
        Action::ThreadTaskFollowup {
            followup,
            route_id,
            grant_revision,
            ..
        } => {
            match crate::mobile::thread_followup::handle(
                shared,
                &request.machine_id,
                device,
                token,
                followup,
                Some((route_id, *grant_revision)),
            ) {
                Ok(result) => success(&request.request_id, result),
                Err("unauthorized") => anyhow::bail!("relay permission changed"),
                Err(code) => failure(&request.request_id, code),
            }
        }
        Action::ThreadTaskFollowupReceipt { followup, .. } => {
            match crate::mobile::thread_followup::handle(
                shared,
                &request.machine_id,
                device,
                token,
                followup,
                None,
            ) {
                Ok(result) => success(&request.request_id, result),
                Err("unauthorized") => anyhow::bail!("relay permission changed"),
                Err(code) => failure(&request.request_id, code),
            }
        }
        Action::ThreadTaskAccept {
            decision,
            route_id,
            grant_revision,
            ..
        } => {
            match crate::mobile::thread_task_acceptance::handle(
                shared,
                &request.machine_id,
                device,
                token,
                decision,
                Some((route_id, *grant_revision)),
            ) {
                Ok(result) => success(&request.request_id, result),
                Err("unauthorized") => anyhow::bail!("relay permission changed"),
                Err(code) => failure(&request.request_id, code),
            }
        }
        Action::ThreadTaskTerminalControl {
            request: scope,
            operation,
            lookup_only,
            ..
        } => {
            match crate::mobile::thread_terminal_control::handle(
                shared,
                &request.machine_id,
                (device, token),
                scope,
                operation,
                *lookup_only,
                terminal,
            ) {
                Ok(result) => success(&request.request_id, result),
                Err("unauthorized") => anyhow::bail!("relay permission changed"),
                Err(code) => failure(&request.request_id, code),
            }
        }
        Action::ThreadTaskTerminal { request: scope, .. } => {
            match crate::mobile::thread_task_terminal::read(
                shared,
                &request.machine_id,
                device,
                token,
                scope,
            ) {
                Ok(result) => success(&request.request_id, result),
                Err("unauthorized") => anyhow::bail!("relay permission changed"),
                Err(code) => failure(&request.request_id, code),
            }
        }
        Action::ThreadTaskAcceptance { decision, .. } => {
            match crate::mobile::thread_task_acceptance::handle(
                shared,
                &request.machine_id,
                device,
                token,
                decision,
                None,
            ) {
                Ok(result) => success(&request.request_id, result),
                Err("unauthorized") => anyhow::bail!("relay permission changed"),
                Err(code) => failure(&request.request_id, code),
            }
        }
        Action::ThreadTaskReview { request: scope, .. } => {
            match crate::mobile::thread_task_review::read(
                shared,
                &request.machine_id,
                device,
                token,
                scope,
            ) {
                Ok(result) => success(&request.request_id, result),
                Err("unauthorized") => anyhow::bail!("relay permission changed"),
                Err(code) => failure(&request.request_id, code),
            }
        }
        Action::ThreadTask {
            task,
            route_id,
            grant_revision,
            ..
        } => {
            match crate::mobile::thread_task::submit(
                shared,
                &request.machine_id,
                device,
                token,
                *task.clone(),
                route_id.clone(),
                *grant_revision,
            ) {
                Ok(result) => success(&request.request_id, result),
                Err("unauthorized") => anyhow::bail!("relay permission changed"),
                Err(code) => failure(&request.request_id, code),
            }
        }
        Action::ThreadTaskReceipt {
            collaboration_workspace_id,
            thread_id,
            operation_id,
            ..
        } => {
            match crate::mobile::thread_task::receipt(
                shared,
                &request.machine_id,
                device,
                token,
                collaboration_workspace_id,
                thread_id,
                operation_id,
            ) {
                Ok(result) => success(&request.request_id, result),
                Err("unauthorized") => anyhow::bail!("relay permission changed"),
                Err(code) => failure(&request.request_id, code),
            }
        }
        Action::ThreadMessage { .. }
        | Action::ThreadReceipt { .. }
        | Action::ThreadEvents { .. } => {
            match super::thread_delivery::handle(shared, &request, device, token) {
                Ok(result) => success(&request.request_id, result),
                Err("unauthorized") => anyhow::bail!("relay permission changed"),
                Err(code) => failure(&request.request_id, code),
            }
        }
        Action::Snapshot { workspace_id, .. } => {
            match crate::mobile::snapshot::read(
                shared,
                &request.machine_id,
                device,
                token,
                workspace_id.as_deref(),
            ) {
                Ok(result) => success(&request.request_id, result),
                Err("unauthorized") => anyhow::bail!("relay permission changed"),
                Err(code) => failure(&request.request_id, code),
            }
        }
        _ => unreachable!("allowlist checked before authentication"),
    };
    Ok(serde_json::to_vec(&reply)?)
}
