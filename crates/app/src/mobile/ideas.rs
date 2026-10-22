//! 工作空间想法的只读视图：与项目计划读取同一授权闸门，不写任何东西，也不进入 GPUI 线程。
//! 字段限定为截断后的标题与粗粒度状态；正文、项目路径、任务引用和控制能力都不下发。

use super::server::Shared;
use serde_json::{json, Value};

pub(super) fn read(
    shared: &Shared,
    machine: &str,
    device: &str,
    token: &str,
    workspace: &str,
) -> Result<Value, &'static str> {
    if !super::command::valid_id(workspace) {
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
    let ideas = storage::IdeasStore::new(access.store.path().to_path_buf())
        .mobile_view(workspace)
        .map_err(|_| "unavailable")?;
    Ok(json!({"status":"workspace_ideas","workspace_id":workspace,"ideas":ideas}))
}
