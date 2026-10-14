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
        ("dispatch_task", "派工到指定卡片，不绕过审批", json!({"operation_id":{"type":"string"},"card_id":{"type":"string"},"card_revision":{"type":"string"},"task":{"type":"string","minLength":1,"maxLength":8000},"context":{"type":"string","maxLength":8000},"allow_queue":{"type":"boolean","default":true}}),json!(["operation_id","card_id","card_revision","task"])),
        ("cancel_task", "取消指定连接器任务，不替用户决定审批",json!({"operation_id":{"type":"string"},"task_id":{"type":"string"}}),json!(["operation_id","task_id"])),
        ("reply_agent", "向已有卡片发送会话消息，不创建任务",json!({"operation_id":{"type":"string"},"card_id":{"type":"string"},"card_revision":{"type":"string"},"message":{"type":"string","minLength":1,"maxLength":8000}}),json!(["operation_id","card_id","card_revision","message"])),
        ("create_card", "直接在指定区域创建Claude或Codex卡片，可一次附带任务，无需再次确认",json!({"operation_id":{"type":"string"},"workspace_id":{"type":"string"},"area_id":{"type":"string"},"profile":{"type":"string","enum":["claude","codex"]},"task":{"type":"string","minLength":1,"maxLength":8000}}),json!(["operation_id","workspace_id","area_id","profile"])),
    ].into_iter().map(|(name, description, properties, required)| {
        let mut schema = json!({"type":"object","properties":properties,"required":required,"additionalProperties":false});
        if name == "get_task_progress" { schema["oneOf"] = json!([{"required":["task_id"]},{"required":["operation_id"]}]); }
        let mut tool = Tool::new(name, description, schema.as_object().expect("object schema").clone());
        tool.annotations = Some(ToolAnnotations::new().read_only(!super::write::is_write(name)).destructive(false).idempotent(true).open_world(false));
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
        "workspace_closed" => "目标工作空间未打开",
        "area_not_found" => "项目区域不存在",
        "card_not_found" => "目标卡片不存在",
        "card_not_loaded" => "目标卡片未加载",
        "profile_unavailable" => "仅支持 Claude 和 Codex",
        "card_create_failed" => "创建卡片失败",
        "operation_conflict" => "同一操作编号已用于其它参数",
        "approval_pending" => "卡片正在等待审批",
        "input_occupied" => "卡片有用户未提交输入",
        "card_busy" => "卡片正在忙碌，当前请求不允许排队",
        "untracked_requires_confirmation" => "卡片没有可信跟踪，请在 Norma 中确认",
        "not_connector_task" => "当前授权不能取消其它来源的任务",
        "task_too_long" => "文本最多为 8000 字符",
        "agent_rejected" => "Agent 未接收此操作",
        "already_submitted" => "任务已提交，无法撤回",
        "outcome_unknown" => "结果待核对，请勿重复提交",
        "task_not_found" => "任务不存在",
        "target_changed" => "卡片身份已改变",
        "rate_limited" => "调用过于频繁，请稍后重试",
        "unauthorized" => "连接授权已失效",
        "connector_disabled" => "连接器已关闭",
        _ => "读取失败，请重试",
    };
    let mut value = json!({"error":{"code":code,"message":message,"retryable":matches!(code,"rate_limited"|"internal"|"workspace_closed")}});
    if let Some(seconds) = retry_after {
        value["error"]["retry_after_s"] = json!(seconds);
    }
    value
}
