use super::server::Shared;
use norma::mobile::{ApprovalListRequest, ApprovalSnapshot};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

pub struct LiveApprovalRequest {
    pub request: ApprovalListRequest,
    pub deadline: Instant,
    pub reply: crossbeam_channel::Sender<Result<Vec<ApprovalSnapshot>, &'static str>>,
}

pub(super) fn read(
    shared: &Shared,
    machine: &str,
    device: &str,
    token: &str,
    request: ApprovalListRequest,
) -> Result<Value, &'static str> {
    read_until(
        shared,
        machine,
        device,
        token,
        request,
        Instant::now() + Duration::from_secs(4),
    )
}

pub(super) fn read_until(
    shared: &Shared,
    machine: &str,
    device: &str,
    token: &str,
    request: ApprovalListRequest,
    deadline: Instant,
) -> Result<Value, &'static str> {
    let deadline = deadline.min(Instant::now() + Duration::from_secs(4));
    if Instant::now() >= deadline {
        return Err("unavailable");
    }
    if !request.valid() {
        return Err("invalid_request");
    }
    let before = {
        let access = shared.access().map_err(|_| "unavailable")?;
        let grant = access
            .authenticate(machine, device, token)
            .map_err(|_| "unauthorized")?
            .ok_or("unauthorized")?;
        if !grant.allows(&request.workspace_id, false) {
            return Err("unauthorized");
        }
        grant.revision
    };
    let source = shared.approvals.as_ref().ok_or("unavailable")?;
    let (reply, result) = crossbeam_channel::bounded(1);
    // 4s leaves 1s of the iOS exchange budget for wire delivery. Read-only timeouts may be retried.
    source
        .try_send(LiveApprovalRequest {
            request: request.clone(),
            deadline,
            reply,
        })
        .map_err(|_| "unavailable")?;
    let items = result
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| "unavailable")??;
    let access = shared.access().map_err(|_| "unavailable")?;
    let current = access
        .authenticate(machine, device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    if current.revision != before || !current.allows(&request.workspace_id, false) {
        return Err("unauthorized");
    }
    let result = json!({"status":"approvals","approvals":{"request":request,"items":items}});
    // Same 500 KiB envelope as project reads; never silently truncate a large approval request.
    if result.to_string().len() > 500 * 1024 {
        return Err("approval_too_large");
    }
    Ok(result)
}
