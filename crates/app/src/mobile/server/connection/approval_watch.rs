use super::{conversation, failure, success, Action, Request, Shared};
use norma::mobile::{ApprovalListRequest, MobileSocket};
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
    let Action::WatchApprovals { device_id, token } = &request.action else {
        return Err("invalid_request");
    };
    let revision = {
        let access = shared.access().map_err(|_| "unavailable")?;
        let grant = access
            .authenticate(&request.machine_id, device_id, token)
            .map_err(|_| "unauthorized")?
            .ok_or("unauthorized")?;
        // Same access→sockets lock order as revoke: a newly authenticated stream cannot be missed.
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
    // ponytail: one <=500KiB complete approval projection per peer. Add per-project deltas only
    // if measured approval payload traffic dominates; heartbeats never repeat tool input.
    let mut previous: Option<Value> = None;
    let mut sequence = 0u64;
    let mut sent_at = Instant::now();
    let waiter = super::watch_wait::WatchWait::new(socket)?;
    loop {
        let changed_at = crate::mobile::project_events::revision();
        let snapshot = read(shared, &request.machine_id, device_id, token, revision)?;
        let changed = previous.as_ref() != Some(&snapshot);
        if changed || sent_at.elapsed() >= Duration::from_secs(2) {
            if changed && previous.is_some() {
                sequence = sequence.checked_add(1).ok_or("stream_exhausted")?;
            }
            let response = success(
                &request.request_id,
                json!({"status":"approval_event",
                "approval_event":{"stream_id":stream_id,"sequence":sequence,
                    "snapshot":if changed { Some(&snapshot) } else { None }}}),
            );
            if response.to_string().len() > 500 * 1024 {
                return Err("approval_too_large");
            }
            conversation::send(socket, response)?;
            sent_at = Instant::now();
            previous = Some(snapshot);
        }
        waiter.wait(socket, shared, changed_at, sent_at)?;
    }
}

fn read(
    shared: &Shared,
    machine: &str,
    device: &str,
    token: &str,
    revision: u64,
) -> Result<Value, &'static str> {
    // One4s budget shared by catalog, all open-project reads and final route check; leaves1s
    // of the native5s first-frame budget for wire delivery. Never multiply4s by project count.
    let deadline = Instant::now() + Duration::from_secs(4);
    let catalog =
        crate::mobile::snapshot::read_until(shared, machine, device, token, None, deadline)?;
    if catalog["grant"]["revision"].as_u64() != Some(revision) {
        return Err("grant_changed");
    }
    let mut projects = Vec::new();
    for project in catalog["projects"].as_array().ok_or("unavailable")? {
        let mut items = json!([]);
        let mut error = None;
        if project["state"] == "open" {
            let scope = ApprovalListRequest {
                workspace_id: project["workspace_id"]
                    .as_str()
                    .ok_or("unavailable")?
                    .into(),
                route_id: project["route_id"].as_str().ok_or("unavailable")?.into(),
            };
            match crate::mobile::approval::read_until(
                shared, machine, device, token, scope, deadline,
            ) {
                Ok(mut response) => items = response["approvals"]["items"].take(),
                Err("unauthorized") => return Err("unauthorized"),
                Err(code) => error = Some(code),
            }
        }
        projects.push(json!({"project":project,"items":items,"error":error}));
    }
    // A route/permission change while collecting another project must discard earlier tool input.
    let current =
        crate::mobile::snapshot::read_until(shared, machine, device, token, None, deadline)?;
    if current["grant"] != catalog["grant"] {
        return Err("grant_changed");
    }
    let verified = current["projects"].as_array().ok_or("unavailable")?;
    if projects.len() != verified.len() || Instant::now() >= deadline {
        return Err("unavailable");
    }
    for (source, project) in projects.iter_mut().zip(verified) {
        if ["workspace_id", "state", "route_id"]
            .iter()
            .any(|key| source["project"][key] != project[key])
        {
            return Err("unavailable");
        }
        // Adding an empty card or renaming the project does not invalidate its route. Keep the
        // verified current metadata without forcing every other approval to reconnect.
        source["project"] = project.clone();
    }
    Ok(json!({"grant":current["grant"],"projects":projects}))
}
