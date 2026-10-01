use super::{command, server::Shared, thread_task_review, MobileCommand, MobileCommandRequest};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::{atomic::Ordering, Arc};
use storage::{MobileCommandReservation, MobileCommandStore, NewMobileCommand};

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

/// Reads and writes share the original task/delivery checks. The optional attempt supplies only
/// current routing/authorization; neither value can change an already reserved decision.
pub(super) fn handle(
    shared: &Arc<Shared>,
    machine: &str,
    device: &str,
    token: &str,
    decision: &Decision,
    attempt: Option<(&str, u64)>,
) -> Result<Value, &'static str> {
    if !command::valid_id(&decision.operation_id)
        || attempt.is_some_and(|(route, revision)| !command::valid_id(route) || revision == 0)
    {
        return Err("invalid_request");
    }
    let access = shared.access().map_err(|_| "unavailable")?;
    let grant = access
        .authenticate(machine, device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    if let Some((_, revision)) = attempt {
        if grant.revision != revision {
            return Err("grant_changed");
        }
    }
    let path = access.store.path();
    let (original, task, delivery) =
        thread_task_review::resolve(shared, path, machine, device, &decision.target, &grant)?;
    if attempt.is_some() && !grant.allows(&task.project_id, true) {
        return Err("unauthorized");
    }
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
    } else if let Some((route, revision)) = attempt {
        if acceptance.is_some() {
            return Err("delivery_accepted");
        }
        shared.commands.as_ref().ok_or("unavailable")?;
        let record = NewMobileCommand {
            device_id: device.into(),
            operation_id: decision.operation_id.clone(),
            workspace_id: task.project_id.clone(),
            route_id: route.into(),
            owner_id: shared.owner_id.clone(),
            request_hash: fingerprint,
        };
        let lease = shared.command_lease(device, &decision.operation_id)?;
        let result = match store
            .reserve_thread_command(&record, &binding)
            .map_err(|_| "unavailable")?
        {
            MobileCommandReservation::Conflict => return Err("idempotency_conflict"),
            MobileCommandReservation::Existing(receipt) => {
                lease.finished.store(true, Ordering::Release);
                command::receipt_response(shared, *receipt)
            }
            MobileCommandReservation::New => command::dispatch_reserved(
                shared,
                MobileCommandRequest {
                    operation_id: decision.operation_id.clone(),
                    workspace_id: task.project_id,
                    route_id: route.into(),
                    grant_revision: revision,
                    command: MobileCommand::AcceptDelivery {
                        key: delivery.outcome.key,
                        fingerprint: target.fingerprint.clone(),
                    },
                },
                record,
                store.clone(),
                lease,
            )?,
        };
        Some(result["receipt"].clone())
    } else {
        None
    };
    Ok(
        json!({"status":"thread_task_acceptance", "thread_acceptance":{
            "decision":decision,"receipt":receipt,"acceptance":acceptance
        }}),
    )
}
