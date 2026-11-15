//! 大脑没有权限闸门：适配器的每个权限请求都直接放行。
use super::{
    client::AcpError,
    trace::{ToolTrace, TraceItem, TraceKind},
    wire::Connection,
};
use serde_json::{json, Value};

pub(super) fn answer(
    conn: &mut Connection,
    id: &Value,
    method: &str,
    params: &Value,
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
    let mut item = TraceItem::new(
        TraceKind::Permission,
        "权限请求：放行",
        format!("{tool}：{wanted}"),
    );
    item.tool = call["toolCallId"].as_str().map(|id| {
        Box::new(ToolTrace {
            id: id.into(),
            name: Some(tool.into()),
            input: Some(wanted.into()),
            description: None,
            status: Some("in_progress".into()),
        })
    });
    sink(item);
    let outcome = match pick_allow(params) {
        Some(option_id) => json!({"outcome":"selected", "optionId":option_id}),
        None => json!({"outcome":"cancelled"}),
    };
    conn.respond(id, json!({"outcome":outcome}))
        .map_err(|_| AcpError::Closed)
}

/// 优先本次放行，适配器没有提供时退到永久放行选项。
fn pick_allow(params: &Value) -> Option<String> {
    let options = params["options"].as_array()?;
    ["allow_once", "allow_always"].iter().find_map(|wanted| {
        options.iter().find_map(|option| {
            (option["kind"] == *wanted)
                .then(|| option["optionId"].as_str().map(str::to_string))
                .flatten()
        })
    })
}
