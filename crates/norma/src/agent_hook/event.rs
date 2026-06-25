use super::{profile_for_source, AgentHookError, AgentHookEvent, AgentHookTaskState, HookSource};
use crate::protocol::NodeId;
use serde_json::Value;

pub fn parse_hook_event(
    source: HookSource,
    node_id: NodeId,
    raw_json: Value,
) -> Result<AgentHookEvent, AgentHookError> {
    let provider_event = string_field(&raw_json, "hook_event_name")
        .or_else(|| string_field(&raw_json, "event_name"))
        .ok_or(AgentHookError::MissingEventName)?;
    Ok(AgentHookEvent {
        node_id,
        profile: profile_for_source(source),
        provider_event,
        session_id: string_field(&raw_json, "session_id"),
        turn_id: string_field(&raw_json, "turn_id"),
        tool_name: string_field(&raw_json, "tool_name"),
        message: string_field(&raw_json, "message")
            .or_else(|| string_field(&raw_json, "error"))
            .or_else(|| string_field(&raw_json, "reason")),
        raw_json,
    })
}

pub fn task_state_for_provider_event(event: &AgentHookEvent) -> Option<AgentHookTaskState> {
    match event.provider_event.as_str() {
        "SessionStart" => Some(AgentHookTaskState::Ready),
        "UserPromptSubmit" => Some(AgentHookTaskState::Thinking),
        "PreToolUse" => Some(AgentHookTaskState::RunningTool {
            tool_name: event.tool_name.clone(),
        }),
        "PostToolUse" | "SubagentStop" => Some(AgentHookTaskState::Thinking),
        "PermissionRequest" => Some(AgentHookTaskState::WaitingApproval {
            tool_name: event.tool_name.clone(),
        }),
        "PostToolUseFailure" | "StopFailure" => Some(AgentHookTaskState::Failed {
            message: event
                .message
                .clone()
                .unwrap_or_else(|| event.provider_event.clone()),
        }),
        "Stop" => Some(AgentHookTaskState::Done),
        "SessionEnd" => Some(AgentHookTaskState::Ended),
        "SubagentStart" => Some(AgentHookTaskState::RunningTool {
            tool_name: Some("subagent".to_string()),
        }),
        _ => None,
    }
}

pub fn hook_events_for_source(source: HookSource) -> &'static [&'static str] {
    match source {
        HookSource::Claude => &[
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "PermissionRequest",
            "PostToolUseFailure",
            "StopFailure",
            "Stop",
            "SessionEnd",
            "SubagentStart",
            "SubagentStop",
        ],
        HookSource::Codex => &[
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "PermissionRequest",
            "Stop",
            "SubagentStart",
            "SubagentStop",
        ],
    }
}

fn string_field(raw: &Value, field: &str) -> Option<String> {
    raw.get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
