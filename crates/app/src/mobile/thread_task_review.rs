use super::{command, server::Shared, thread_task::authorized_task};
use base64::{engine::general_purpose::STANDARD, Engine};
use norma::mobile::ThreadTask;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use storage::{
    MobileCommandStore, MobileDeviceGrant, MobileTaskDeliveryStore, StoredThreadCommand,
    TaskDeliveryContent, TaskDeliverySummary,
};

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Target {
    pub collaboration_workspace_id: String,
    pub thread_id: String,
    pub operation_id: String,
    pub task_ref: String,
    pub fingerprint: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub collaboration_workspace_id: String,
    pub thread_id: String,
    pub operation_id: String,
    pub task_ref: String,
    pub fingerprint: String,
    pub offset: usize,
}

/// Background-only, immutable read. Holding the authorization owner prevents a revoke/read race.
pub(super) fn read(
    shared: &Shared,
    machine: &str,
    device: &str,
    token: &str,
    request: &Request,
) -> Result<Value, &'static str> {
    if request.offset >= norma::mobile::MAX_CONVERSATION_BYTES {
        return Err("invalid_request");
    }
    let access = shared.access().map_err(|_| "unavailable")?;
    let grant = access
        .authenticate(machine, device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    let path = access.store.path();
    let target = Target {
        collaboration_workspace_id: request.collaboration_workspace_id.clone(),
        thread_id: request.thread_id.clone(),
        operation_id: request.operation_id.clone(),
        task_ref: request.task_ref.clone(),
        fingerprint: request.fingerprint.clone(),
    };
    let (bound, task, summary) = resolve(shared, path, machine, device, &target, &grant)?;
    let saved = MobileTaskDeliveryStore::new(path.to_path_buf())
        .get(&task.project_id, &summary.outcome.key)
        .map_err(|_| "unavailable")?
        .ok_or("delivery_unavailable")?;
    if saved.fingerprint != request.fingerprint {
        return Err("not_found");
    }
    let TaskDeliveryContent::Available { review } = saved.content else {
        return Err("delivery_unavailable");
    };
    // ponytail: re-read the bounded frozen record per chunk; add a per-stream cache only if measured
    // large-review latency warrants it. Never cache authorization or read the mutable working tree.
    let payload = serde_json::to_vec(&json!({
        "request":{"workspace_id":task.project_id,"route_id":bound.receipt.route_id,"delivery_key":saved.outcome.key},
        "review":review,"delivery":{"outcome":saved.outcome,"fingerprint":saved.fingerprint,"saved_at_unix":saved.saved_at_unix}
    })).map_err(|_| "unavailable")?;
    if payload.len() > norma::mobile::MAX_CONVERSATION_BYTES {
        return Err("review_too_large");
    }
    if request.offset >= payload.len() {
        return Err("invalid_request");
    }
    // Base64(160KiB) < 214KiB, leaving >42KiB for request/encryption within the 256KiB relay limit.
    let end = (request.offset + 160 * 1024).min(payload.len());
    Ok(json!({"status":"thread_task_review","thread_review":{
        "request":request,"total_bytes":payload.len(),"data":STANDARD.encode(&payload[request.offset..end])
    }}))
}

/// Caller retains the access guard through its read or revocable command reservation.
pub(super) fn resolve(
    shared: &Shared,
    path: &std::path::Path,
    machine: &str,
    device: &str,
    request: &Target,
    grant: &MobileDeviceGrant,
) -> Result<(StoredThreadCommand, ThreadTask, TaskDeliverySummary), &'static str> {
    if ![
        &request.collaboration_workspace_id,
        &request.thread_id,
        &request.operation_id,
    ]
    .into_iter()
    .all(|id| command::valid_id(id))
        || request.task_ref.len() > 160
        || request.task_ref.is_empty()
        || request.fingerprint.len() != 64
        || !request.fingerprint.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("invalid_request");
    }
    let bound = MobileCommandStore::new(path.to_path_buf())
        .thread_command(
            device,
            machine,
            &request.collaboration_workspace_id,
            &request.thread_id,
            &request.operation_id,
            &shared.owner_id,
        )
        .map_err(|_| "unavailable")?
        .ok_or("not_found")?;
    let task = authorized_task(path, machine, device, &bound, grant)?;
    let dispatch = bound.receipt.result.as_ref().ok_or("not_found")?;
    if bound.receipt.state != "succeeded" || dispatch["task_ref"] != request.task_ref {
        return Err("not_found");
    }
    let deliveries = MobileTaskDeliveryStore::new(path.to_path_buf());
    let summary = deliveries
        .for_command(device, &request.operation_id, &task.project_id)
        .map_err(|_| "unavailable")?
        .ok_or("delivery_unavailable")?;
    if summary.fingerprint != request.fingerprint
        || dispatch["node_id"].as_u64() != Some(summary.outcome.key.node_id)
    {
        return Err("not_found");
    }
    Ok((bound, task, summary))
}
