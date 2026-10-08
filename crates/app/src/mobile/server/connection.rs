use super::Shared;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{sync::Arc, time::Instant};
mod listener;
#[cfg(test)]
#[path = "../../../tests/unit/mobile_legacy_retirement_tests.rs"]
mod mobile_legacy_retirement_tests;
#[cfg(test)]
#[path = "../../../tests/unit/workspace_inbox_api_tests.rs"]
mod workspace_inbox_api_tests;
#[cfg(test)]
#[path = "../../../tests/unit/workspace_terminal_tests.rs"]
mod workspace_terminal_tests;
pub(super) use listener::spawn;
mod approval_watch;
mod archive;
mod conversation;
mod conversation_watch;
mod push;
pub(super) mod realtime;
mod record_watch;
pub(super) mod relay;
mod review;
mod stream;
mod thread_delivery;
mod watch_wait;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u32,
    request_id: String,
    machine_id: String,
    action: Action,
}
mod action;
mod legacy;
mod workspace;
pub(in crate::mobile) use action::Action;
use workspace::workspace_credentials;
mod image;
mod terminal;

fn decode(text: &str) -> Result<Request, Value> {
    legacy::reject_mutation(text)?;
    let Ok(request) = serde_json::from_str::<Request>(text) else {
        return Err(failure("", "invalid_request"));
    };
    let id = &request.request_id;
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(failure("", "invalid_request"));
    }
    if request.version != 1 {
        return Err(failure(id, "unsupported_version"));
    }
    Ok(request)
}

fn handle(request: Request, shared: &Arc<Shared>, socket_id: u64) -> Value {
    let id = &request.request_id;
    if let Some((device, token)) = workspace_credentials(&request.action) {
        return match crate::mobile::workspace_chat::handle(
            shared,
            &request.machine_id,
            device,
            token,
            &request.action,
        ) {
            Ok(result) => success(id, result),
            Err(code) => failure(id, code),
        };
    }
    if let Some((device, token)) = legacy::credentials(&request.action) {
        return match legacy::read(shared, &request, device, token) {
            Ok(result) => success(id, result),
            Err(code) => failure(id, code),
        };
    }
    if matches!(
        request.action,
        Action::WorkspaceActivity { .. }
            | Action::WorkspaceTerminal { .. }
            | Action::WorkspaceAgentCreate { .. }
            | Action::WorkspaceAgentCreationReceipt { .. }
            | Action::WorkspaceCatalog { .. }
            | Action::WorkspaceMessage { .. }
            | Action::WorkspaceReceipt { .. }
            | Action::WorkspaceEvents { .. }
            | Action::WorkspaceConfigure { .. }
            | Action::ThreadReceipt { .. }
            | Action::ThreadEvents { .. }
            | Action::ThreadTaskReceipt { .. }
            | Action::ThreadTaskReview { .. }
            | Action::ThreadTaskAcceptance { .. }
            | Action::ThreadTaskTerminal { .. }
    ) {
        return failure(id, "unsupported_transport");
    }
    if let Action::PairingCode { message } = &request.action {
        // Before identity is known, ONLY a public SPAKE2 message is accepted. This returns
        // encrypted bootstrap data and never tags the socket as authorized.
        if !request.machine_id.is_empty() {
            return failure(id, "invalid_request");
        }
        return match shared
            .access()
            .and_then(|mut access| access.exchange_code(message, Instant::now()))
        {
            Ok(reply) => success(
                id,
                json!({"status":"code_offer", "message":reply.message, "sealed":reply.sealed}),
            ),
            Err(_) => failure(id, "pairing_unavailable"),
        };
    }
    let write_result = match &request.action {
        Action::PushSubscription { .. } | Action::SetPushSubscription { .. } => {
            Some(push::handle(shared, &request))
        }
        Action::TaskDeliveries { .. } => Some(review::list(shared, &request)),
        Action::SessionArchive { .. } => Some(archive::list(shared, &request)),
        Action::ProjectPlan {
            device_id,
            token,
            workspace_id,
            plan_id,
        } => Some(crate::mobile::todo::read(
            shared,
            &request.machine_id,
            device_id,
            token,
            workspace_id,
            Some(plan_id.as_str()),
        )),
        Action::ProjectPlans {
            device_id,
            token,
            workspace_id,
            before,
        } => Some(crate::mobile::plan::read(
            shared,
            &request.machine_id,
            device_id,
            token,
            workspace_id,
            *before,
        )),
        Action::Approvals {
            device_id,
            token,
            request: approvals,
        } => Some(crate::mobile::approval::read(
            shared,
            &request.machine_id,
            device_id,
            token,
            approvals.clone(),
        )),
        Action::Command {
            device_id,
            token,
            request: command,
        } => Some(crate::mobile::command::submit(
            shared,
            &request.machine_id,
            device_id,
            token,
            command.as_ref().clone(),
        )),
        Action::QueuedMessage {
            device_id,
            token,
            workspace_id,
            operation_id,
        } => Some(crate::mobile::queue::status(
            shared,
            &request.machine_id,
            device_id,
            token,
            workspace_id,
            operation_id,
            None,
        )),
        Action::CancelQueuedMessage {
            device_id,
            token,
            workspace_id,
            operation_id,
            grant_revision,
        } => Some(crate::mobile::queue::status(
            shared,
            &request.machine_id,
            device_id,
            token,
            workspace_id,
            operation_id,
            Some(*grant_revision),
        )),
        Action::RealtimeOperations {
            device_id,
            token,
            workspace_id,
            after,
        } => Some(realtime::operations(
            shared,
            &request.machine_id,
            device_id,
            token,
            workspace_id,
            *after,
        )),
        Action::Receipt {
            device_id,
            token,
            workspace_id,
            operation_id,
        } => Some(crate::mobile::command::receipt(
            shared,
            &request.machine_id,
            device_id,
            token,
            workspace_id,
            operation_id,
        )),
        _ => None,
    };
    if let Some(result) = write_result {
        return match result {
            Ok(result) => success(id, result),
            Err(code) => failure(id, code),
        };
    }
    if let Action::Snapshot {
        device_id,
        token,
        workspace_id,
    } = &request.action
    {
        return match crate::mobile::snapshot::read(
            shared,
            &request.machine_id,
            device_id,
            token,
            workspace_id.as_deref(),
        ) {
            Ok(result) => success(id, result),
            Err(code) => failure(id, code),
        };
    }
    let Ok(mut access) = shared.access() else {
        return failure(id, "unavailable");
    };
    if request.machine_id != access.machine_id() {
        return failure(id, "unauthorized");
    }
    match request.action {
        Action::WorkspaceActivity { .. }
        | Action::WorkspaceTerminal { .. }
        | Action::WorkspaceAgentCreate { .. }
        | Action::WorkspaceAgentCreationReceipt { .. }
        | Action::WorkspaceCatalog { .. }
        | Action::WorkspaceMessage { .. }
        | Action::WorkspaceReceipt { .. }
        | Action::WorkspaceEvents { .. }
        | Action::WorkspaceConfigure { .. } => {
            unreachable!("workspace actions handled before authorization lock")
        }
        Action::CloudRegistration {
            device_id,
            token,
            cloud_device_id,
            token_hash,
        } => {
            let allowed = access
                .authenticate(&request.machine_id, &device_id, &token)
                .ok()
                .flatten()
                .is_some_and(|grant| {
                    grant.all_projects_access == Some(storage::MobilePermission::Control)
                        || grant
                            .projects
                            .iter()
                            .any(|p| p.access == storage::MobilePermission::Control)
                });
            if !allowed {
                return failure(id, "unauthorized");
            }
            match access
                .identity
                .cloud_registration(&device_id, &cloud_device_id, &token_hash)
            {
                Ok(proof) => success(
                    id,
                    json!({"status":"cloud_registration", "registration_proof":proof}),
                ),
                Err(_) => failure(id, "invalid_request"),
            }
        }
        Action::VoiceAuthorization { device_id, token } => {
            let allowed = access
                .authenticate(&request.machine_id, &device_id, &token)
                .ok()
                .flatten()
                .is_some_and(|grant| {
                    grant
                        .projects
                        .iter()
                        .any(|project| project.access == storage::MobilePermission::Control)
                });
            if !allowed {
                return failure(id, "unauthorized");
            }
            match access.identity.voice_authorization(&device_id) {
                Ok(authorization) => success(
                    id,
                    json!({"status":"voice_authorized", "voice_authorization":authorization}),
                ),
                Err(_) => failure(id, "unavailable"),
            }
        }
        Action::ThreadTaskFollowupReceipt { .. }
        | Action::ThreadTaskReceipt { .. }
        | Action::ThreadTaskReview { .. }
        | Action::ThreadTaskTerminal { .. }
        | Action::ThreadTaskAcceptance { .. }
        | Action::ThreadReceipt { .. }
        | Action::ThreadEvents { .. }
        | Action::UploadImage { .. }
        | Action::RealtimeOperations { .. }
        | Action::BindRealtime { .. }
        | Action::ChatImage { .. }
        | Action::Terminal { .. }
        | Action::PairingCode { .. }
        | Action::WatchPlan { .. }
        | Action::WatchApprovals { .. }
        | Action::PushSubscription { .. }
        | Action::SetPushSubscription { .. }
        | Action::SessionArchive { .. }
        | Action::ArchivedConversation { .. }
        | Action::TaskDeliveries { .. }
        | Action::ProjectReview { .. }
        | Action::WatchProject { .. }
        | Action::ProjectPlans { .. }
        | Action::ProjectPlan { .. }
        | Action::WatchConversation { .. }
        | Action::Approvals { .. }
        | Action::Conversation { .. }
        | Action::Snapshot { .. }
        | Action::Command { .. }
        | Action::QueuedMessage { .. }
        | Action::CancelQueuedMessage { .. }
        | Action::Receipt { .. } => {
            unreachable!("snapshot handled before acquiring authorization lock")
        }
        Action::Pair { request } => match access.claim(request, Instant::now()) {
            Ok(pending) => success(id, json!({"status":"pending","pending":pending})),
            Err(_) => failure(id, "pairing_unavailable"),
        },
        Action::Authenticate { device_id, token } => {
            match access.authenticate(&request.machine_id, &device_id, &token) {
                Ok(Some(grant)) => {
                    // Hold access until socket tagging is complete. Revoke takes the locks in the
                    // same order, so it cannot miss an authentication that completed just before it.
                    let mut sockets = shared.sockets.lock().unwrap_or_else(|e| e.into_inner());
                    let Some(socket) = sockets.get_mut(&socket_id) else {
                        return failure(id, "unavailable");
                    };
                    socket.device_id = Some(device_id);
                    let capabilities: Vec<&str> = if shared.workspaces.is_some() {
                        vec!["workspace_terminal_v1"]
                    } else {
                        Vec::new()
                    };
                    success(
                        id,
                        json!({"status":"authorized","grant":grant,"capabilities":capabilities}),
                    )
                }
                _ => failure(id, "unauthorized"),
            }
        }
    }
}

fn failure(id: &str, code: &str) -> Value {
    json!({"version":1,"request_id":id,"error":code})
}
fn success(id: &str, result: Value) -> Value {
    json!({"version":1,"request_id":id,"result":result})
}
