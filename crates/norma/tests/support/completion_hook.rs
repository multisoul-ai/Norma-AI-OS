use norma::agent_hook::{
    parse_hook_event, CompletionAction, CompletionJudge, HookSource, CONFIRMATION_WINDOW_MS,
};
use serde_json::{json, Value};

pub fn judge() -> CompletionJudge {
    CompletionJudge::new()
}

pub fn claude(node: u64, raw: Value) -> norma::agent_hook::AgentHookEvent {
    parse_hook_event(HookSource::Claude, node, raw).expect("Claude hook JSON 应可解析")
}

pub fn background_bash(_node: u64, task_id: &str, tool_use_id: &str) -> Value {
    json!({
        "hook_event_name": "PostToolUse",
        "session_id": "session-a",
        "tool_name": "Bash",
        "tool_use_id": tool_use_id,
        "tool_input": {"command": format!("sleep 2; echo {task_id}"), "run_in_background": true},
        "tool_response": {"backgroundTaskId": task_id}
    })
}

pub fn stop_with_tasks(_node: u64, ids: &[&str]) -> Value {
    let tasks: Vec<_> = ids
        .iter()
        .map(|id| json!({"id": id, "type": "shell", "status": "running"}))
        .collect();
    json!({
        "hook_event_name": "Stop",
        "session_id": "session-a",
        "stop_hook_active": false,
        "background_tasks": tasks,
        "session_crons": [{"id": "loop-1", "status": "running"}]
    })
}

pub fn empty_stop() -> Value {
    json!({
        "hook_event_name": "Stop",
        "session_id": "session-a",
        "stop_hook_active": false,
        "background_tasks": [],
        "session_crons": [{"id": "loop-1", "status": "running"}]
    })
}

pub fn notification(task_id: &str, tool_use_id: &str, status: &str) -> Value {
    json!({
        "hook_event_name": "UserPromptSubmit",
        "session_id": "session-a",
        "prompt": format!(
            "<task-notification><task-id>{task_id}</task-id><tool-use-id>{tool_use_id}</tool-use-id><status>{status}</status></task-notification>"
        )
    })
}

pub fn confirm_flags(actions: &[CompletionAction]) -> Vec<bool> {
    actions
        .iter()
        .filter_map(|action| match action {
            CompletionAction::Confirm {
                background_unsettled,
                event,
            } => {
                assert_eq!(
                    event.provider_event, "Stop",
                    "确认动作必须重放原来的 Stop，下游才认完成"
                );
                Some(*background_unsettled)
            }
            _ => None,
        })
        .collect()
}

pub fn quiet_until_confirm(judge: &mut CompletionJudge, started_ms: u64) -> Vec<bool> {
    let early = started_ms + CONFIRMATION_WINDOW_MS - 1;
    let due = started_ms + CONFIRMATION_WINDOW_MS;
    let too_early = confirm_flags(&judge.poll(early));
    assert!(
        too_early.is_empty(),
        "确认窗口差 1ms（{early}）不得 Done，实际 {too_early:?}"
    );
    confirm_flags(&judge.poll(due))
}
