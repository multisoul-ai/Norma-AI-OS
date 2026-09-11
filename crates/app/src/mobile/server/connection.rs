use super::{ActiveSocket, Shared};
use norma::mobile::accept_mobile_websocket;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    net::TcpStream,
    sync::{atomic::Ordering, Arc},
    time::{Duration, Instant},
};
use tungstenite::Message;
mod approval_watch;
mod archive;
mod conversation;
mod conversation_watch;
mod push;
mod record_watch;
mod review;
mod stream;
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
use action::Action;
mod terminal;

struct SocketGuard {
    shared: Arc<Shared>,
    id: u64,
}
impl Drop for SocketGuard {
    fn drop(&mut self) {
        self.shared
            .sockets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.id);
    }
}

pub(super) fn spawn(socket: TcpStream, tls: Arc<rustls::ServerConfig>, shared: Arc<Shared>) {
    let id = shared.next_socket.fetch_add(1, Ordering::Relaxed);
    {
        let mut sockets = shared.sockets.lock().unwrap_or_else(|e| e.into_inner());
        // Eight simultaneous handshakes/connections across ALL interfaces; unauthenticated peers
        // get the same limit. Each handshake has 3s and each message (including idle) has 30s.
        if shared.stopped.load(Ordering::Acquire) || sockets.len() >= 8 {
            return;
        }
        let Ok(stream) = socket.try_clone() else {
            return;
        };
        sockets.insert(
            id,
            ActiveSocket {
                stream,
                device_id: None,
            },
        );
    }
    let guard = SocketGuard {
        shared: Arc::clone(&shared),
        id,
    };
    let _ = std::thread::Builder::new()
        .name("norma-mobile-peer".into())
        .spawn(move || {
            let _guard = guard;
            let Ok(mut ws) =
                accept_mobile_websocket(socket, tls, Instant::now() + Duration::from_secs(3))
            else {
                return;
            };
            // Pings and invalid requests cannot occupy an unauthenticated slot forever.
            // Pairing may outlive this connection; the phone reconnects with its saved token.
            let auth_deadline = Instant::now() + Duration::from_secs(10);
            while !shared.stopped.load(Ordering::Acquire) {
                let authenticated = shared
                    .sockets
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&id)
                    .is_some_and(|socket| socket.device_id.is_some());
                let deadline = if authenticated {
                    Instant::now() + Duration::from_secs(30)
                } else {
                    auth_deadline
                };
                if Instant::now() >= deadline {
                    break;
                }
                ws.get_mut().sock.set_deadline(deadline);
                let Ok(message) = ws.read() else {
                    break;
                };
                let reply = match message {
                    Message::Text(text) => match decode(&text) {
                        Ok(request) if matches!(request.action, Action::Terminal { .. }) => {
                            terminal::serve(&mut ws, &shared, id, request);
                            break;
                        }
                        Ok(request)
                            if matches!(
                                request.action,
                                Action::WatchPlan { .. }
                                    | Action::TaskDeliveries { watch: true, .. }
                            ) =>
                        {
                            record_watch::serve(&mut ws, &shared, id, request);
                            break;
                        }
                        Ok(request) if matches!(request.action, Action::WatchApprovals { .. }) => {
                            approval_watch::serve(&mut ws, &shared, id, request);
                            break;
                        }
                        Ok(request) if matches!(request.action, Action::WatchProject { .. }) => {
                            stream::serve(&mut ws, &shared, id, request);
                            break;
                        }
                        Ok(request)
                            if matches!(request.action, Action::WatchConversation { .. }) =>
                        {
                            conversation_watch::serve(&mut ws, &shared, id, request);
                            break;
                        }
                        Ok(request) if matches!(request.action, Action::ProjectReview { .. }) => {
                            review::serve(&mut ws, &shared, id, request);
                            break;
                        }
                        Ok(request)
                            if matches!(
                                request.action,
                                Action::Conversation { .. } | Action::ArchivedConversation { .. }
                            ) =>
                        {
                            conversation::serve(&mut ws, &shared, id, request);
                            break;
                        }
                        Ok(request) => handle(request, &shared, id),
                        Err(reply) => reply,
                    },
                    Message::Ping(_) => {
                        if ws.flush().is_err() {
                            break;
                        }
                        continue;
                    }
                    Message::Pong(_) => continue,
                    _ => break,
                };
                if shared.stopped.load(Ordering::Acquire) {
                    break;
                }
                ws.get_mut()
                    .sock
                    .set_deadline(Instant::now() + Duration::from_secs(3));
                if ws.send(Message::text(reply.to_string())).is_err() {
                    break;
                }
            }
        });
}

fn decode(text: &str) -> Result<Request, Value> {
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
        Action::Terminal { .. }
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
                    success(id, json!({"status":"authorized","grant":grant}))
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
