//! 主会话和审批会话的权限请求直接放行。幕僚会话只放行 `norma brain` 和项目内 Read。
use super::staff_shell::{command_text, read_inside_project, staff_command_allowed};
use super::{
    client::AcpError,
    trace::{ToolTrace, TraceItem, TraceKind},
    wire::Connection,
};
use serde_json::{json, Value};
use std::path::Path;

pub(super) fn answer(
    conn: &mut Connection,
    id: &Value,
    method: &str,
    params: &Value,
    staff: Option<(&Path, &Path)>,
    sink: &mut dyn FnMut(TraceItem),
) -> Result<(), AcpError> {
    if method != "session/request_permission" {
        return conn
            .respond_error(id, -32601, "unsupported")
            .map_err(|_| AcpError::Closed);
    }
    let call = &params["toolCall"];
    let raw = &call["rawInput"];
    let wanted = raw["command"]
        .as_str()
        .or_else(|| raw["file_path"].as_str())
        .or_else(|| raw["path"].as_str())
        .unwrap_or_default();
    let tool = call["name"]
        .as_str()
        .or(call["kind"].as_str())
        .unwrap_or("tool");
    let allowed = staff.is_none_or(|(norma_bin, cwd)| staff_allows(tool, raw, norma_bin, cwd));
    let mut item = TraceItem::new(
        TraceKind::Permission,
        if allowed {
            "权限请求：放行"
        } else {
            "权限请求：拒绝"
        },
        if allowed {
            format!("{tool}：{wanted}")
        } else {
            format!("{tool}：{wanted}\n请派 Worker 去做")
        },
    );
    item.tool = call["toolCallId"].as_str().map(|id| {
        Box::new(ToolTrace {
            id: id.into(),
            name: Some(tool.into()),
            input: Some(wanted.into()),
            description: None,
            status: Some("in_progress".into()),
            path: None,
        })
    });
    sink(item);
    let outcome = if allowed {
        match pick_kind(params, &["allow_once", "allow_always"]) {
            Some(option_id) => json!({"outcome":"selected", "optionId":option_id}),
            None => json!({"outcome":"cancelled"}),
        }
    } else {
        match pick_kind(params, &["reject_once", "reject_always"]) {
            Some(option_id) => json!({"outcome":"selected", "optionId":option_id}),
            None => json!({"outcome":"cancelled"}),
        }
    };
    conn.respond(id, json!({"outcome":outcome}))
        .map_err(|_| AcpError::Closed)
}

fn staff_allows(tool: &str, raw: &Value, norma_bin: &Path, cwd: &Path) -> bool {
    if command_text(raw).is_some() || is_shell(tool) {
        return staff_command_allowed(raw, norma_bin);
    }
    if is_read(tool) {
        let path = raw["file_path"]
            .as_str()
            .or_else(|| raw["path"].as_str())
            .unwrap_or("");
        return read_inside_project(path, cwd);
    }
    false
}

fn is_shell(tool: &str) -> bool {
    matches!(
        tool.to_ascii_lowercase().as_str(),
        "bash" | "execute" | "terminal" | "powershell"
    )
}

fn is_read(tool: &str) -> bool {
    tool.eq_ignore_ascii_case("read") || tool.eq_ignore_ascii_case("read file")
}

/// 按 kind 的优先顺序选适配器给出的选项。
fn pick_kind(params: &Value, kinds: &[&str]) -> Option<String> {
    let options = params["options"].as_array()?;
    kinds.iter().find_map(|wanted| {
        options.iter().find_map(|option| {
            (option["kind"] == *wanted)
                .then(|| option["optionId"].as_str().map(str::to_string))
                .flatten()
        })
    })
}
