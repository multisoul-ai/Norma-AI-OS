use super::{
    command::{dispatch_reserved, receipt_response, valid_id},
    server::Shared,
    MobileCommand, MobileCommandRequest,
};
use agent::PlanTodoChoice;
use serde_json::{json, Value};
use std::{sync::atomic::Ordering, sync::Arc};
use storage::{MobileCommandStore, NewMobileCommand, PlanTodoReservation, ProjectPlanStore};

pub(super) fn submit(
    shared: &Arc<Shared>,
    machine: &str,
    device: &str,
    token: &str,
    mut request: MobileCommandRequest,
    record: NewMobileCommand,
) -> Result<Value, &'static str> {
    let MobileCommand::DecidePlanTodo {
        plan_id,
        todo_index,
        choice,
        retry_of,
    } = &request.command
    else {
        return Err("invalid_request");
    };
    let store = {
        let access = shared.access().map_err(|_| "unavailable")?;
        let grant = access
            .authenticate(machine, device, token)
            .map_err(|_| "unauthorized")?
            .ok_or("unauthorized")?;
        if !grant.allows(&request.workspace_id, true) {
            return Err("unauthorized");
        }
        if grant.revision != request.grant_revision {
            return Err("grant_changed");
        }
        MobileCommandStore::new(access.store.path().into())
    };
    let existing = store
        .get(
            device,
            &request.operation_id,
            &request.workspace_id,
            &shared.owner_id,
        )
        .map_err(|_| "unavailable")?
        .is_some();
    if !existing {
        if let PlanTodoChoice::Accept { target, queue } = choice {
            let current =
                super::queue::live_target(shared, machine, device, token, &request, target)?;
            if !queue && current != **target {
                return Err("card_changed");
            }
        }
    }
    // Hold the same authorization gate as normal commands through atomic reservation and dispatch.
    let access = shared.access().map_err(|_| "unavailable")?;
    let grant = access
        .authenticate(machine, device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    if !grant.allows(&request.workspace_id, true) {
        return Err("unauthorized");
    }
    if grant.revision != request.grant_revision {
        return Err("grant_changed");
    }
    if !existing {
        if let PlanTodoChoice::Accept {
            target,
            queue: false,
        } = choice
        {
            if let Some(run) = &target.run_id {
                if store
                    .has_pending_queued_message(&record, target.node_id, run)
                    .map_err(|_| "unavailable")?
                {
                    return Err("queue_pending");
                }
            }
        }
    }
    let lease = shared.command_lease(device, &request.operation_id)?;
    let reservation = match retry_of {
        Some(previous) => store.retry_plan_todo(
            &record,
            plan_id,
            *todo_index,
            choice,
            request.grant_revision,
            previous,
        ),
        None => store.reserve_plan_todo(
            &record,
            plan_id,
            *todo_index,
            choice,
            request.grant_revision,
        ),
    }
    .map_err(|_| "invalid_todo")?;
    match reservation {
        PlanTodoReservation::Conflict => Err("idempotency_conflict"),
        PlanTodoReservation::AlreadyDecided => Err("todo_decided"),
        PlanTodoReservation::Existing(receipt) => {
            lease.finished.store(true, Ordering::Release);
            Ok(receipt_response(shared, receipt))
        }
        PlanTodoReservation::New(todo) => {
            super::project_events::changed();
            if matches!(choice, PlanTodoChoice::Accept { queue: false, .. }) {
                request.command = MobileCommand::SendMessage {
                    expected: todo.target,
                    text: todo.instruction,
                };
                dispatch_reserved(shared, request, record, store, lease)
            } else {
                // Reject is already persisted; queued acceptance is handled by the existing Mac worker.
                lease.finished.store(true, Ordering::Release);
                let receipt = store
                    .get(
                        device,
                        &request.operation_id,
                        &request.workspace_id,
                        &shared.owner_id,
                    )
                    .map_err(|_| "unavailable")?
                    .ok_or("unavailable")?;
                Ok(receipt_response(shared, receipt))
            }
        }
    }
}

pub(super) fn read(
    shared: &Shared,
    machine: &str,
    device: &str,
    token: &str,
    workspace: &str,
    plan_id: Option<&str>,
) -> Result<Value, &'static str> {
    if !valid_id(workspace) || plan_id.is_some_and(|id| !valid_id(id)) {
        return Err("invalid_request");
    }
    let access = shared.access().map_err(|_| "unavailable")?;
    let grant = access
        .authenticate(machine, device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    if !grant.allows(workspace, false) {
        return Err("unauthorized");
    }
    let path = access.store.path().to_path_buf();
    let plans = ProjectPlanStore::new(path.clone());
    let plan = match plan_id {
        Some(id) => Some(
            plans
                .get(workspace, id)
                .map_err(|_| "unavailable")?
                .ok_or("not_found")?,
        ),
        // One newest entry, ordered by the existing monotonic SQLite sequence; no directory scan.
        None => plans
            .list(workspace, None, 1)
            .map_err(|_| "unavailable")?
            .items
            .into_iter()
            .next(),
    };
    let Some(plan) = plan else {
        return Ok(
            json!({"status":"project_plan","workspace_id":workspace,"grant":grant,"plan":null,"decisions":[]}),
        );
    };
    let plan_id = plan.plan_id.as_str();
    let decisions = MobileCommandStore::new(path.clone())
        .plan_todo_decisions(workspace, plan_id, &shared.owner_id)
        .map_err(|_| "unavailable")?;
    let outcomes = storage::MobileTurnStore::for_workspace(path, workspace.into());
    let decisions = decisions
        .into_iter()
        .map(|decision| {
            let outcome = sent_turn(&decision)
                .map(|key| outcomes.get(&key))
                .transpose()
                .map_err(|_| "unavailable")?
                .flatten();
            Ok(
                json!({"todo_index":decision.todo_index,"choice":decision.choice,
            "receipt":receipt_response(shared,decision.receipt)["receipt"],
            "queued_message":decision.queued.map(super::queue::response),"outcome":outcome,
            "retry_of":decision.retry_of,"can_retry":decision.can_retry}),
            )
        })
        .collect::<Result<Vec<_>, &'static str>>()?;

    let result = json!({"status":"project_plan","workspace_id":workspace,"grant":grant,"plan":plan,"decisions":decisions});
    // One80KiB plan plus eight4KiB receipts/8KiB messages remain inside the500KiB wire limit.
    if result.to_string().len() > 500 * 1024 {
        return Err("plan_too_large");
    }
    Ok(result)
}

/// Only an actual sending receipt can name the Provider turn to look up. A queue acknowledgement
/// or declined proposal never owns execution evidence, even if the target card currently says Done.
fn sent_turn(decision: &storage::StoredTodoDecision) -> Option<storage::MobileTurnKey> {
    let PlanTodoChoice::Accept { target, queue } = &decision.choice else {
        return None;
    };
    let result = if *queue {
        let queued = decision.queued.as_ref().filter(|q| q.state == "sent")?;
        queued.result.as_ref()?
    } else {
        if decision.receipt.state != "succeeded" {
            return None;
        }
        decision.receipt.result.as_ref()?
    };
    let key = storage::MobileTurnKey {
        node_id: result["node_id"].as_u64()?,
        profile: target.profile.clone(),
        run_id: result["run_id"].as_str()?.into(),
        session_id: result["session_id"].as_str()?.into(),
        turn_id: result["turn_id"].as_str()?.into(),
    };
    if key.node_id != target.node_id
        || target.run_id.as_ref().is_some_and(|run| run != &key.run_id)
        || target
            .session_id
            .as_ref()
            .is_some_and(|session| session != &key.session_id)
    {
        return None;
    }
    Some(key)
}
