use super::{failure, success, Action, Request, Shared};
use norma::mobile::{ConversationFragment, ConversationRequest, MobileSocket};
use serde_json::{json, Value};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tungstenite::Message;

pub(super) fn serve(
    socket: &mut MobileSocket,
    shared: &Arc<Shared>,
    socket_id: u64,
    request: Request,
) {
    if let Err(code) = run(socket, shared, socket_id, &request) {
        let _ = send(socket, failure(&request.request_id, code));
    }
}
fn run(
    socket: &mut MobileSocket,
    shared: &Arc<Shared>,
    socket_id: u64,
    request: &Request,
) -> Result<(), &'static str> {
    let (device_id, token, conversation) = target(request)?;
    let archive = match &request.action {
        Action::ArchivedConversation { profile, .. } => Some(storage::MobileSessionKey {
            node_id: conversation.node_id,
            profile: profile.clone(),
            run_id: conversation.run_id.clone(),
            session_id: conversation.session_id.clone(),
        }),
        _ => None,
    };
    let result = crate::mobile::conversation::read(
        shared,
        &request.machine_id,
        device_id,
        token,
        conversation.clone(),
        archive,
    )?;
    send_result(socket, shared, socket_id, request, result, "conversation")
}

pub(super) fn send_result(
    socket: &mut MobileSocket,
    shared: &Arc<Shared>,
    socket_id: u64,
    request: &Request,
    result: Value,
    key: &str,
) -> Result<(), &'static str> {
    let (device_id, token, conversation) = target(request)?;
    if result.to_string().len() <= 500 * 1024 {
        return send(socket, success(&request.request_id, result));
    }
    // ponytail: one frozen Provider page per existing peer, <=32MiB. No cache or resumable blob store;
    // a dropped transfer is discarded and a new read uses the original Provider cursor.
    let payload = result[key].to_string();
    drop(result);
    let revision = verify_live(shared, request, conversation, device_id, token)?;
    {
        let access = shared.access().map_err(|_| "unavailable")?;
        let grant = access
            .authenticate(&request.machine_id, device_id, token)
            .map_err(|_| "unauthorized")?
            .ok_or("unauthorized")?;
        if grant.revision != revision || !grant.allows(&conversation.workspace_id, false) {
            return Err("unauthorized");
        }
        shared
            .sockets
            .lock()
            .map_err(|_| "unavailable")?
            .get_mut(&socket_id)
            .ok_or("unavailable")?
            .device_id = Some(device_id.into());
    }
    let transfer_id =
        crate::norma::endpoint::new_workspace_route_id().map_err(|_| "unavailable")?;
    let mut offset = 0;
    while offset < payload.len() {
        let fragment = ConversationFragment::at(&transfer_id, &payload, offset)?;
        offset += fragment.text.len();
        if offset == payload.len()
            && verify_live(shared, request, conversation, device_id, token)? != revision
        {
            return Err("unauthorized");
        }
        {
            let access = shared.access().map_err(|_| "unavailable")?;
            let current = access
                .authenticate(&request.machine_id, device_id, token)
                .map_err(|_| "unauthorized")?
                .ok_or("unauthorized")?;
            if current.revision != revision || !current.allows(&conversation.workspace_id, false) {
                return Err("unauthorized");
            }
        }
        send(
            socket,
            success(
                &request.request_id,
                json!({"status":"conversation_fragment","fragment":fragment}),
            ),
        )?;
    }
    Ok(())
}
pub(super) fn verify_live(
    shared: &Shared,
    request: &Request,
    conversation: &ConversationRequest,
    device: &str,
    token: &str,
) -> Result<u64, &'static str> {
    let snapshot = crate::mobile::snapshot::read(
        shared,
        &request.machine_id,
        device,
        token,
        Some(&conversation.workspace_id),
    )?;
    let project = &snapshot["projects"][0];
    if project["state"] != "open" || project["route_id"] != conversation.route_id {
        return Err("session_changed");
    }
    if matches!(request.action, Action::ArchivedConversation { .. }) {
        return snapshot["grant"]["revision"].as_u64().ok_or("unauthorized");
    }
    let card = project["cards"]
        .as_array()
        .and_then(|cards| {
            cards
                .iter()
                .find(|card| card["node_id"].as_u64() == Some(conversation.node_id))
        })
        .ok_or("session_changed")?;
    if card["run_id"] != conversation.run_id || card["session_id"] != conversation.session_id {
        return Err("session_changed");
    }
    snapshot["grant"]["revision"].as_u64().ok_or("unauthorized")
}
pub(super) fn send(socket: &mut MobileSocket, response: Value) -> Result<(), &'static str> {
    socket
        .get_mut()
        .sock
        .set_deadline(Instant::now() + Duration::from_secs(3));
    socket
        .send(Message::text(response.to_string()))
        .map_err(|_| "unavailable")
}

fn target(request: &Request) -> Result<(&str, &str, &ConversationRequest), &'static str> {
    match &request.action {
        Action::ArchivedConversation {
            device_id,
            token,
            request,
            ..
        }
        | Action::Conversation {
            device_id,
            token,
            request,
        }
        | Action::WatchConversation {
            device_id,
            token,
            request,
        } => Ok((device_id, token, request)),
        _ => Err("invalid_request"),
    }
}
pub(super) fn authorize(
    shared: &Shared,
    request: &Request,
    socket_id: u64,
) -> Result<u64, &'static str> {
    let (device, token, target) = self::target(request)?;
    let access = shared.access().map_err(|_| "unavailable")?;
    let grant = access
        .authenticate(&request.machine_id, device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    if !grant.allows(&target.workspace_id, false) {
        return Err("unauthorized");
    }
    shared
        .sockets
        .lock()
        .map_err(|_| "unavailable")?
        .get_mut(&socket_id)
        .ok_or("unavailable")?
        .device_id = Some(device.into());
    Ok(grant.revision)
}
