use super::{conversation, failure, success, Action, Request, Shared};
use norma::mobile::MobileSocket;
use serde_json::{json, Value};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub(super) fn serve(
    socket: &mut MobileSocket,
    shared: &Arc<Shared>,
    socket_id: u64,
    request: Request,
) {
    if let Err(code) = run(socket, shared, socket_id, &request) {
        let _ = conversation::send(socket, failure(&request.request_id, code));
    }
}

fn run(
    socket: &mut MobileSocket,
    shared: &Arc<Shared>,
    socket_id: u64,
    request: &Request,
) -> Result<(), &'static str> {
    let (device_id, token, workspace_id) = match &request.action {
        Action::WatchPlan {
            device_id,
            token,
            workspace_id,
            ..
        } => (device_id, token, workspace_id),
        Action::TaskDeliveries {
            watch: true,
            device_id,
            token,
            request: scope,
            ..
        } => (device_id, token, &scope.workspace_id),
        _ => return Err("invalid_request"),
    };
    let revision = {
        let access = shared.access().map_err(|_| "unavailable")?;
        let grant = access
            .authenticate(&request.machine_id, device_id, token)
            .map_err(|_| "unauthorized")?
            .ok_or("unauthorized")?;
        if !grant.allows(workspace_id, false) {
            return Err("unauthorized");
        }
        // Same access→sockets lock order as revoke; revocation cannot miss a new stream.
        shared
            .sockets
            .lock()
            .map_err(|_| "unavailable")?
            .get_mut(&socket_id)
            .ok_or("unavailable")?
            .device_id = Some(device_id.clone());
        grant.revision
    };
    let stream_id = crate::norma::endpoint::new_workspace_route_id().map_err(|_| "unavailable")?;
    // ponytail: one bounded saved-record snapshot per peer; shared signals coalesce changes. Introduce
    // record deltas only if measured traffic warrants it. Heartbeats contain no record text.
    let mut previous: Option<Value> = None;
    let mut sequence = 0u64;
    let mut sent_at = Instant::now();
    let waiter = super::watch_wait::WatchWait::new(socket)?;
    loop {
        let changed_at = crate::mobile::project_events::revision();
        // Reuse historical reads: a closed project can still expose its authorized saved plan.
        let snapshot = match &request.action {
            Action::WatchPlan { plan_id, .. } => crate::mobile::todo::read(
                shared,
                &request.machine_id,
                device_id,
                token,
                workspace_id,
                plan_id.as_deref(),
            )?,
            Action::TaskDeliveries { .. } => super::review::list(shared, request)?,
            _ => return Err("invalid_request"),
        };
        if snapshot["grant"]["revision"].as_u64() != Some(revision) {
            return Err("grant_changed");
        }
        let changed = previous.as_ref() != Some(&snapshot);
        if changed || sent_at.elapsed() >= Duration::from_secs(2) {
            if changed && previous.is_some() {
                sequence = sequence.checked_add(1).ok_or("stream_exhausted")?;
            }
            let body = if changed { Some(&snapshot) } else { None };
            let result = match &request.action {
                Action::WatchPlan { plan_id, .. } => json!({"status":"plan_event", "plan_event":{
                    "workspace_id":workspace_id,"plan_id":plan_id,"stream_id":stream_id,"sequence":sequence,"snapshot":body}}),
                Action::TaskDeliveries { .. } => {
                    json!({"status":"delivery_event", "delivery_event":{
                    "stream_id":stream_id,"sequence":sequence,"snapshot":body}})
                }
                _ => return Err("invalid_request"),
            };
            let response = success(&request.request_id, result);
            if response.to_string().len() > 500 * 1024 {
                return Err("plan_too_large");
            }
            conversation::send(socket, response)?;
            sent_at = Instant::now();
            previous = Some(snapshot);
        }
        waiter.wait(socket, shared, changed_at, sent_at)?;
    }
}
