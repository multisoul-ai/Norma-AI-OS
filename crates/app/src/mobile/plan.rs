use super::server::Shared;
use serde_json::{json, Value};

pub(super) fn read(
    shared: &Shared,
    machine: &str,
    device: &str,
    token: &str,
    workspace: &str,
    before: Option<u64>,
) -> Result<Value, &'static str> {
    if !super::command::valid_id(workspace) || before.is_some_and(|v| v == 0 || v > i64::MAX as u64)
    {
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
    // The small SQLite read shares the authorization gate with revocation; never runs on GPUI.
    let plans = storage::ProjectPlanStore::new(access.store.path().to_path_buf())
        .list(workspace, before, 4)
        .map_err(|_| "unavailable")?;
    let result = json!({"status":"project_plans","workspace_id":workspace,"plans":plans});
    if result.to_string().len() > 500 * 1024 {
        return Err("plan_too_large");
    }
    Ok(result)
}
