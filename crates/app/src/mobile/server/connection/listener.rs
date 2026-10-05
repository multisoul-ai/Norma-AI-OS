use super::{
    approval_watch, conversation, conversation_watch, decode, handle, image, realtime,
    record_watch, review, stream, terminal, Action,
};
use crate::mobile::server::{ActiveSocket, Shared};
use norma::mobile::accept_mobile_websocket;
use std::{
    net::TcpStream,
    sync::{atomic::Ordering, Arc},
    time::{Duration, Instant},
};
use tungstenite::Message;

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

pub(in crate::mobile::server) fn spawn(
    socket: TcpStream,
    tls: Arc<rustls::ServerConfig>,
    shared: Arc<Shared>,
) {
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
                realtime: None,
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
                        Ok(request) if matches!(request.action, Action::BindRealtime { .. }) => {
                            realtime::serve(&mut ws, &shared, id, request);
                            break;
                        }
                        Ok(request)
                            if matches!(
                                request.action,
                                Action::UploadImage { .. } | Action::ChatImage { .. }
                            ) =>
                        {
                            image::serve(&mut ws, &shared, id, request);
                            break;
                        }
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
