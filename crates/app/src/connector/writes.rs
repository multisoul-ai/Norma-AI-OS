//! Worker-side reservation, receipt mapping and recovery; every side effect crosses execution.rs.
use super::{
    execution::{Executed, ExecutionCall},
    repository::{LiveRequest, Repository},
};
use norma::connector::{Snapshot, WriteRequest};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use storage::{
    MobileCommandReceipt, MobileCommandReservation, MobileCommandStore, NewMobileCommand,
};

pub(super) fn run(
    repo: &Repository,
    epoch: &str,
    request: WriteRequest,
    now: i64,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<Value, &'static str> {
    let store = MobileCommandStore::new(repo.database.clone());
    let device = format!("connector:{epoch}");
    store.cleanup_connector(now).map_err(|_| "internal")?;
    if let Some(existing) = store
        .connector_operation(&device, request.operation_id(), &request.fingerprint())
        .map_err(|_| "internal")?
    {
        return replay(existing, &request);
    }
    let disk =
        super::repository::persisted(&repo.database, &repo.signer).map_err(|_| "internal")?;
    let mut snapshot = repo.read_live(disk).map_err(|_| "internal")?;
    let all = store.connector_origins().map_err(|_| "internal")?;
    super::projection::apply(&mut snapshot, &all, &device);
    let origins = store.connector_receipts(&device).map_err(|_| "internal")?;
    let (workspace, area) = target(repo, &request, &snapshot, &origins)?;
    let command = NewMobileCommand {
        device_id: device,
        operation_id: request.operation_id().into(),
        workspace_id: workspace.clone(),
        route_id: request.route().into(),
        owner_id: "grokbot".into(),
        request_hash: request.fingerprint(),
    };
    match store
        .reserve_connector(&command, now)
        .map_err(|_| "internal")?
    {
        MobileCommandReservation::New => {}
        existing => return replay(existing, &request),
    }
    let mut diagnostic = None;
    let mut outcome = match execute(repo, &request, workspace, area, cancellation) {
        Ok(executed) => receipt(repo, &request, executed, now),
        Err(error) => {
            diagnostic = Some(error.to_string());
            Err(super::execution::error_code(
                &error,
                matches!(request, WriteRequest::Create(_)),
            ))
        }
    };
    if outcome == Err("task_not_found") {
        if let WriteRequest::Cancel(args) = &request {
            if let Some(task) = snapshot
                .tasks
                .iter()
                .find(|t| t.task_id == args.task_id && t.terminal())
            {
                outcome = Ok(
                    json!({"operation_id":request.operation_id(),"task_id":args.task_id,"state":"already_terminal","terminal_state":task.state,"requested_at":now,"replayed":false}),
                );
            }
        }
    }
    if store
        .finish_connector(&command, outcome.clone().map_err(str::to_owned))
        .is_err()
    {
        return Ok(unknown(&request, now, false));
    }
    let mut value = outcome.unwrap_or_else(|code| {
        if code == "outcome_unknown" {
            unknown(&request, now, false)
        } else {
            failure(code, &request, false)
        }
    });
    if let Some(diagnostic) = diagnostic {
        value["_adapter_error"] = json!(diagnostic);
    }
    Ok(value)
}
fn target(
    repo: &Repository,
    request: &WriteRequest,
    snapshot: &Snapshot,
    receipts: &[MobileCommandReceipt],
) -> Result<(String, String), &'static str> {
    let (workspace, area) = match request {
        WriteRequest::Create(a) => (a.workspace_id.clone(), a.area_id.clone()),
        WriteRequest::Dispatch(_) | WriteRequest::Reply(_) => {
            let target = repo.signer.verify(request.route())?;
            (target.workspace_id, target.area_id)
        }
        WriteRequest::Cancel(a) => {
            let owned = receipts.iter().any(|r| {
                r.result.as_ref().is_some_and(|v| {
                    v["task_id"] == a.task_id
                        && (v.get("accepted_at").is_some() || v["created_by"] == "grokbot")
                })
            });
            let allow_other = storage::connector_store::ConnectorStore::new(repo.database.clone())
                .config()
                .map_err(|_| "internal")?
                .allow_cancel_other_tasks;
            if !owned && !allow_other {
                return Err("not_connector_task");
            }
            let task = snapshot
                .tasks
                .iter()
                .find(|t| t.task_id == a.task_id)
                .ok_or("task_not_found")?;
            let card = repo.signer.verify(&task.card_id)?;
            (card.workspace_id, card.area_id)
        }
    };
    let space = snapshot
        .workspaces
        .iter()
        .find(|w| w.workspace_id == workspace)
        .ok_or("workspace_not_found")?;
    let project = space
        .projects
        .iter()
        .find(|p| p.area_id == area)
        .ok_or("area_not_found")?;
    if !project.loaded {
        return Err("workspace_closed");
    }
    Ok((workspace, area))
}
fn execute(
    repo: &Repository,
    request: &WriteRequest,
    workspace: String,
    area: String,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<Executed, agent::AdapterError> {
    let disk = super::repository::persisted(&repo.database, &repo.signer)
        .map_err(|_| agent::AdapterError::Unknown("outcome_unknown".into()))?;
    let (reply, receive) = crossbeam_channel::bounded(1);
    let call = ExecutionCall {
        request: request.clone(),
        workspace,
        area,
        deadline: Instant::now() + Duration::from_secs(30),
        cancellation: cancellation.child_token(),
        reply,
    };
    let stop = call.cancellation.clone();
    let (capture, _) = crossbeam_channel::bounded(1);
    let queued = repo.live.try_send(LiveRequest {
        areas: disk.areas,
        births: disk.births,
        snapshot: disk.snapshot,
        reply: capture,
        execute: Some(call),
    });
    if queued.is_err() {
        return Err(agent::AdapterError::Unknown("outcome_unknown".into()));
    }
    let result = receive.recv_timeout(Duration::from_secs(30));
    stop.cancel();
    result.map_err(|_| agent::AdapterError::Unknown("outcome_unknown".into()))?
}
fn receipt(
    repo: &Repository,
    request: &WriteRequest,
    executed: Executed,
    now: i64,
) -> Result<Value, &'static str> {
    if executed.receipt.status == "unknown" {
        return Err("outcome_unknown");
    }
    let operation = request.operation_id();
    match request {
        WriteRequest::Dispatch(a) => {
            let identity = repo.signer.verify(&a.card_id)?;
            let snapshot = repo.snapshot_unscoped().map_err(|_| "outcome_unknown")?;
            let card = snapshot
                .workspaces
                .iter()
                .flat_map(|w| &w.cards)
                .find(|c| c.card_id == a.card_id)
                .ok_or("outcome_unknown")?;
            Ok(
                json!({"operation_id":operation,"task_id":executed.task_ref.as_ref().ok_or("outcome_unknown")?,"card_id":a.card_id,"role_name":card.role_name,"workspace_id":identity.workspace_id,"state":dispatch_state(&executed),"tracking":executed.tracking,"replayed":false,"accepted_at":now}),
            )
        }
        WriteRequest::Reply(a) => Ok(
            json!({"operation_id":operation,"card_id":a.card_id,"reply_id":executed.receipt.reply_id.ok_or("outcome_unknown")?.to_string(),"reply_status":executed.receipt.reply_status.ok_or("outcome_unknown")?,"replayed":false}),
        ),
        WriteRequest::Cancel(a) => {
            let terminal = executed.receipt.reply_status;
            Ok(
                json!({"operation_id":operation,"task_id":a.task_id,"state":if terminal.is_some(){"already_terminal"}else if executed.receipt.status == "succeeded"{"cancelled"}else{"cancel_requested"},"terminal_state":terminal,"requested_at":now,"replayed":false}),
            )
        }
        WriteRequest::Create(a) => {
            let node_id = executed.receipt.node_id.ok_or("card_create_failed")?;
            let snapshot = repo.snapshot_unscoped().map_err(|_| "outcome_unknown")?;
            let card = snapshot
                .workspaces
                .iter()
                .filter(|w| w.workspace_id == a.workspace_id)
                .flat_map(|w| &w.cards)
                .find(|c| c.identity.area_id == a.area_id && c.identity.node_id == node_id)
                .ok_or("card_create_failed")?;
            let mut value = json!({"operation_id":operation,"card_id":card.card_id,"card_revision":card.card_revision,"role_name":card.role_name,"profile":a.profile,"workspace_id":a.workspace_id,"area_id":a.area_id,"created_at":now,"state":"loaded","created_by":"grokbot","replayed":false});
            if let Some(task) = &executed.task_ref {
                value["task_id"] = json!(task);
                value["task_state"] = json!(dispatch_state(&executed));
                value["tracking"] = json!(executed.tracking);
            }
            Ok(value)
        }
    }
}
fn dispatch_state(executed: &Executed) -> &str {
    match executed.task_state.as_deref() {
        Some("queued") => "queued",
        Some("running") => {
            if executed.tracking.as_deref() == Some("untracked") {
                "untracked"
            } else {
                "submitted"
            }
        }
        _ => "accepted",
    }
}
fn replay(
    reservation: MobileCommandReservation,
    request: &WriteRequest,
) -> Result<Value, &'static str> {
    match reservation {
        MobileCommandReservation::Conflict => Err("operation_conflict"),
        MobileCommandReservation::Existing(receipt) => {
            let mut value = receipt.result.clone().unwrap_or_else(|| {
                if receipt.state == "outcome_unknown" {
                    unknown(request, receipt.created_at_unix, true)
                } else {
                    failure(
                        receipt.error.as_deref().unwrap_or("internal"),
                        request,
                        true,
                    )
                }
            });
            value["replayed"] = json!(true);
            Ok(value)
        }
        MobileCommandReservation::New => Err("internal"),
    }
}
fn failure(code: &str, request: &WriteRequest, replayed: bool) -> Value {
    let mut value = norma::connector::tool_error(code);
    value["operation_id"] = json!(request.operation_id());
    value["replayed"] = json!(replayed);
    value
}
fn unknown(request: &WriteRequest, now: i64, replayed: bool) -> Value {
    let mut value = json!({"operation_id":request.operation_id(),"state":"outcome_unknown","replayed":replayed});
    let key = match request {
        WriteRequest::Dispatch(_) => "accepted_at",
        WriteRequest::Cancel(_) => "requested_at",
        WriteRequest::Create(_) => "created_at",
        WriteRequest::Reply(_) => return value,
    };
    value[key] = json!(now);
    value
}

#[cfg(test)]
#[path = "../../tests/unit/connector_receipt_tests.rs"]
mod receipt_tests;
