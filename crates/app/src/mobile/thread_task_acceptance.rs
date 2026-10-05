use super::{command, server::Shared, thread_task_review};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use storage::MobileCommandStore;

#[derive(Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Decision {
    #[serde(rename = "type")]
    kind: Kind,
    pub operation_id: String,
    pub target: thread_task_review::Target,
}

#[derive(Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Kind {
    AcceptDelivery,
}

/// Read historical acceptance; never reserve or dispatch a legacy decision.
pub(super) fn handle(
    shared: &Arc<Shared>,
    machine: &str,
    device: &str,
    token: &str,
    decision: &Decision,
) -> Result<Value, &'static str> {
    if !command::valid_id(&decision.operation_id) {
        return Err("invalid_request");
    }
    let access = shared.access().map_err(|_| "unavailable")?;
    let grant = access
        .authenticate(machine, device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    let path = access.store.path();
    let (original, task, delivery) =
        thread_task_review::resolve(shared, path, machine, device, &decision.target, &grant)?;
    let store = MobileCommandStore::new(path.to_path_buf());
    let mut binding = original.binding;
    binding.payload = serde_json::to_string(decision).map_err(|_| "invalid_request")?;
    let fingerprint = hex::encode(Sha256::digest(
        serde_json::to_vec(&("Norma thread acceptance v1", machine, decision))
            .map_err(|_| "invalid_request")?,
    ));
    let target = &decision.target;
    let existing = store
        .thread_command(
            device,
            machine,
            &target.collaboration_workspace_id,
            &target.thread_id,
            &decision.operation_id,
            &shared.owner_id,
        )
        .map_err(|_| "unavailable")?;
    let acceptance = store
        .delivery_acceptance(&task.project_id, &delivery.outcome.key)
        .map_err(|_| "unavailable")?;
    if acceptance
        .as_ref()
        .is_some_and(|a| a.fingerprint != target.fingerprint)
    {
        return Err("unavailable");
    }
    let receipt = if let Some(existing) = existing {
        if existing.binding != binding || existing.fingerprint != fingerprint {
            return Err("idempotency_conflict");
        }
        Some(command::receipt_response(shared, existing.receipt)["receipt"].clone())
    } else {
        None
    };
    Ok(
        json!({"status":"thread_task_acceptance", "thread_acceptance":{
            "decision":decision,"receipt":receipt,"acceptance":acceptance
        }}),
    )
}
