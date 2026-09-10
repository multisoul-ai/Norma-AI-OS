use super::{failure, success, Action, Request, Shared};
use norma::mobile::{MobileSocket, ProjectChange, ProjectProjection, ProjectSnapshot};
use serde_json::{json, Value};
use std::{
    sync::{atomic::Ordering, Arc},
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
    let Action::WatchProject {
        device_id,
        token,
        workspace_id,
    } = &request.action
    else {
        return Err("invalid_request");
    };
    if workspace_id.is_empty()
        || workspace_id.len() > 128
        || !workspace_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("invalid_request");
    }
    let grant_revision = {
        let access = shared.access().map_err(|_| "unavailable")?;
        let grant = access
            .authenticate(&request.machine_id, device_id, token)
            .map_err(|_| "unauthorized")?
            .ok_or("unauthorized")?;
        if !grant.allows(workspace_id, false) {
            return Err("unauthorized");
        }
        // Same lock order as revoke: access → sockets. Revocation cannot miss the new watch.
        let mut sockets = shared.sockets.lock().map_err(|_| "unavailable")?;
        sockets.get_mut(&socket_id).ok_or("unavailable")?.device_id = Some(device_id.clone());
        grant.revision
    };
    let stream_id = crate::norma::endpoint::new_workspace_route_id().map_err(|_| "unavailable")?;
    let mut projection: Option<ProjectProjection> = None;
    let mut sent_at = Instant::now();
    let waiter = super::watch_wait::WatchWait::new(socket)?;
    loop {
        if shared.stopped.load(Ordering::Acquire) {
            return Err("unavailable");
        }
        let revision = crate::mobile::project_events::revision();
        // All IO and serialization stay on this background peer; the GPUI owner only copies metadata.
        let mut result = crate::mobile::snapshot::read(
            shared,
            &request.machine_id,
            device_id,
            token,
            Some(workspace_id),
        )?;
        if result["grant"]["revision"].as_u64() != Some(grant_revision) {
            return Err("unauthorized");
        }
        if result["projects"].as_array().map(Vec::len) != Some(1) {
            return Err("unavailable");
        }
        let project: ProjectSnapshot =
            serde_json::from_value(result["projects"][0].take()).map_err(|_| "unavailable")?;
        let event = match &mut projection {
            Some(current) => current
                .update(project)?
                .unwrap_or_else(|| current.heartbeat()),
            None => {
                let current = ProjectProjection::new(stream_id.clone(), project);
                let event = current.snapshot();
                projection = Some(current);
                event
            }
        };
        if !matches!(&event.change, ProjectChange::Heartbeat)
            || sent_at.elapsed() >= Duration::from_secs(2)
        {
            send(
                socket,
                success(
                    &request.request_id,
                    json!({"status":"project_event","grant":result["grant"].take(),"event":event}),
                ),
            )?;
            sent_at = Instant::now();
        }
        waiter.wait(socket, shared, revision, sent_at)?;
    }
}

fn send(socket: &mut MobileSocket, response: Value) -> Result<(), &'static str> {
    let text = response.to_string();
    if text.len() > 500 * 1024 {
        return Err("snapshot_too_large");
    }
    socket
        .get_mut()
        .sock
        .set_deadline(Instant::now() + Duration::from_secs(3));
    socket.send(Message::text(text)).map_err(|_| "unavailable")
}
