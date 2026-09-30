use super::{command, server::Shared, terminal, thread_task::authorized_task, LiveTerminalTask};
use norma::mobile::TerminalRequest;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use storage::{MobileCommandStore, MobileTaskBaselineStore};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub collaboration_workspace_id: String,
    pub thread_id: String,
    pub operation_id: String,
    pub task_ref: String,
    pub before: Option<usize>,
    pub revision: Option<u64>,
}

/// Background reads resolve immutable identity; the existing foreground channel reads the original PTY.
pub(super) fn read(
    shared: &Shared,
    machine: &str,
    device: &str,
    token: &str,
    request: &Request,
) -> Result<Value, &'static str> {
    let (scope, target, grant_revision) = resolve(shared, machine, device, token, request)?;
    let mut page = terminal::read_task(shared, scope.clone(), Some(target.clone()))?;
    reauthorize(shared, machine, device, token, grant_revision)?;
    // This capability only reads. PTY readiness alone never grants exclusive input ownership.
    page.input_ready = false;
    response(request, &scope, &target, page)
}

pub(super) fn resolve(
    shared: &Shared,
    machine: &str,
    device: &str,
    token: &str,
    request: &Request,
) -> Result<(TerminalRequest, LiveTerminalTask, u64), &'static str> {
    if ![
        &request.collaboration_workspace_id,
        &request.thread_id,
        &request.operation_id,
    ]
    .into_iter()
    .all(|id| command::valid_id(id))
        || request.task_ref.is_empty()
        || request.task_ref.len() > 160
        || request.before.is_some() != request.revision.is_some()
        || request
            .before
            .is_some_and(|before| before == 0 || before > 100_000)
    {
        return Err("invalid_request");
    }
    let access = shared.access().map_err(|_| "unavailable")?;
    let grant = access
        .authenticate(machine, device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    let (scope, target) = resolve_authorized(
        access.store.path(),
        &grant,
        machine,
        device,
        &shared.owner_id,
        request,
    )?;
    Ok((scope, target, grant.revision))
}

/// The background queue already owns a currently authorized grant; no token is stored in its records.
pub(super) fn resolve_authorized(
    path: &std::path::Path,
    grant: &storage::MobileDeviceGrant,
    machine: &str,
    device: &str,
    owner: &str,
    request: &Request,
) -> Result<(TerminalRequest, LiveTerminalTask), &'static str> {
    let bound = MobileCommandStore::new(path.to_path_buf())
        .thread_command(
            device,
            machine,
            &request.collaboration_workspace_id,
            &request.thread_id,
            &request.operation_id,
            owner,
        )
        .map_err(|_| "unavailable")?
        .ok_or("not_found")?;
    let task = authorized_task(path, machine, device, &bound, grant)?;
    let dispatch = bound
        .receipt
        .result
        .as_ref()
        .ok_or("terminal_unavailable")?;
    if bound.receipt.state != "succeeded" || dispatch["task_ref"] != request.task_ref {
        return Err("not_found");
    }
    let key = MobileTaskBaselineStore::new(path.to_path_buf())
        .turn_for_command(
            device,
            &request.operation_id,
            &task.project_id,
            &bound.receipt.route_id,
        )
        .map_err(|_| "unavailable")?
        .ok_or("terminal_unavailable")?;
    if dispatch["node_id"].as_u64() != Some(key.node_id) {
        return Err("not_found");
    }
    let scope = TerminalRequest {
        workspace_id: task.project_id,
        route_id: bound.receipt.route_id,
        node_id: key.node_id,
        run_id: key.run_id.clone(),
        session_id: Some(key.session_id.clone()),
        before: request.before,
        revision: request.revision,
    };
    let target = LiveTerminalTask {
        task_ref: request.task_ref.clone(),
        agent_definition_id: task.agent_definition_id,
        key,
    };
    Ok((scope, target))
}

pub(super) fn reauthorize(
    shared: &Shared,
    machine: &str,
    device: &str,
    token: &str,
    revision: u64,
) -> Result<(), &'static str> {
    let access = shared.access().map_err(|_| "unavailable")?;
    let grant = access
        .authenticate(machine, device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    if grant.revision != revision {
        return Err("unauthorized");
    }
    Ok(())
}

pub(super) fn response(
    request: &Request,
    scope: &TerminalRequest,
    target: &LiveTerminalTask,
    page: norma::mobile::TerminalPage,
) -> Result<Value, &'static str> {
    let result = json!({"status":"thread_task_terminal", "thread_terminal":{
        "request":request, "scope":scope, "key":target.key, "page":page,
    }});
    //256KiB encrypted relay frame -16KiB envelope/escaping margin =240KiB maximum JSON payload.
    if serde_json::to_vec(&result)
        .map_err(|_| "unavailable")?
        .len()
        > 240 * 1024
    {
        return Err("result_too_large");
    }
    Ok(result)
}
