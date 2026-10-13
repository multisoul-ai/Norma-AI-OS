use rmcp::model::{Tool, ToolAnnotations};
use serde_json::{json, Value};
pub(super) fn definitions() -> Vec<Tool> {
    let states = json!([
        "accepted",
        "submitted",
        "running",
        "queued",
        "untracked",
        "cancel_requested",
        "done",
        "failed",
        "cancelled",
        "interrupted",
        "outcome_unknown"
    ]);
    [
        ("directory", "读取工作空间、项目与卡片目录，不加载项目", json!({"workspace_id":{"type":"string"},"include_idle":{"type":"boolean","default":true}}), json!([])),
        ("list_tasks", "读取任务列表", json!({"workspace_id":{"type":"string"},"card_id":{"type":"string"},"states":{"type":"array","items":{"type":"string","enum":states}},"since":{"type":"integer","minimum":0},"cursor":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":50,"default":20}}), json!([])),
        ("get_task_progress", "读取任务可信进度与结果摘要", json!({"task_id":{"type":"string"},"operation_id":{"type":"string"}}), json!([])),
    ].into_iter().map(|(name, description, properties, required)| {
        let mut schema = json!({"type":"object","properties":properties,"required":required,"additionalProperties":false});
        if name == "get_task_progress" { schema["oneOf"] = json!([{"required":["task_id"]},{"required":["operation_id"]}]); }
        let mut tool = Tool::new(name, description, schema.as_object().expect("object schema").clone());
        tool.annotations = Some(ToolAnnotations::new().read_only(true).destructive(false).idempotent(true).open_world(false));
        tool
    }).collect()
}
pub(super) fn valid_states(states: &Option<Vec<String>>) -> bool {
    states.as_ref().is_none_or(|items| {
        !items.is_empty()
            && items.len() <= 11
            && items.iter().all(|s| {
                matches!(
                    s.as_str(),
                    "accepted"
                        | "submitted"
                        | "running"
                        | "queued"
                        | "untracked"
                        | "cancel_requested"
                        | "done"
                        | "failed"
                        | "cancelled"
                        | "interrupted"
                        | "outcome_unknown"
                )
            })
    })
}
pub(super) fn error(code: &str, retry_after: Option<i64>) -> Value {
    let message = match code {
        "invalid_argument" => "参数或游标无效",
        "workspace_not_found" => "工作空间不存在",
        "task_not_found" => "任务不存在",
        "target_changed" => "卡片身份已改变",
        "rate_limited" => "读取过于频繁，请稍后重试",
        "unauthorized" => "连接授权已失效",
        "connector_disabled" => "连接器已关闭",
        _ => "读取失败，请重试",
    };
    let mut value = json!({"error":{"code":code,"message":message,"retryable":matches!(code,"rate_limited"|"internal")}});
    if let Some(seconds) = retry_after {
        value["error"]["retry_after_s"] = json!(seconds);
    }
    value
}
