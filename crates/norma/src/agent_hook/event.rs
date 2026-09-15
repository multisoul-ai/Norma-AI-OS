use super::{profile_for_source, AgentHookError, AgentHookEvent, AgentHookTaskState, HookSource};
use crate::protocol::NodeId;
use serde_json::Value;

pub fn parse_hook_event(
    source: HookSource,
    node_id: NodeId,
    raw_json: Value,
) -> Result<AgentHookEvent, AgentHookError> {
    let provider_event = string_field(&raw_json, "hook_event_name")
        .or_else(|| string_field(&raw_json, "hookEventName"))
        .or_else(|| string_field(&raw_json, "event_name"))
        .ok_or(AgentHookError::MissingEventName)?;
    let provider_event = normalize_provider_event(source, provider_event);
    Ok(AgentHookEvent {
        control_run_id: string_field(&raw_json, "_norma_control_run_id"),
        node_id,
        profile: profile_for_source(source),
        provider_event,
        session_id: string_field(&raw_json, "session_id")
            .or_else(|| string_field(&raw_json, "sessionId"))
            .or_else(|| {
                (source == HookSource::Cursor)
                    .then(|| string_field(&raw_json, "conversation_id"))
                    .flatten()
            }),
        turn_id: string_field(&raw_json, "turn_id")
            .or_else(|| string_field(&raw_json, "turnId"))
            .or_else(|| {
                (source == HookSource::Cursor)
                    .then(|| string_field(&raw_json, "generation_id"))
                    .flatten()
            })
            .or_else(|| {
                (source == HookSource::Claude)
                    .then(|| string_field(&raw_json, "prompt_id"))
                    .flatten()
            }),
        tool_name: string_field(&raw_json, "tool_name")
            .or_else(|| string_field(&raw_json, "toolName")),
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
        "PostToolUseFailure" | "PermissionDenied" => Some(AgentHookTaskState::Thinking),
        "StopFailure" => Some(AgentHookTaskState::Failed {
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
        HookSource::Cursor => &[
            "sessionStart",
            "beforeSubmitPrompt",
            "preToolUse",
            "postToolUse",
            "postToolUseFailure",
            "stop",
            "sessionEnd",
        ],
        HookSource::Grok => &[
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "PostToolUseFailure",
            "PermissionDenied",
            "Notification",
            "Stop",
            "StopFailure",
            "SessionEnd",
            "SubagentStart",
            "SubagentStop",
            "PreCompact",
            "PostCompact",
        ],
        HookSource::OpenCode => &[
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "PostToolUseFailure",
            "Stop",
            "StopFailure",
            "SessionEnd",
        ],
    }
}

fn normalize_provider_event(source: HookSource, provider_event: String) -> String {
    if source != HookSource::Cursor {
        return provider_event;
    }
    match provider_event.as_str() {
        "sessionStart" => "SessionStart",
        "beforeSubmitPrompt" => "UserPromptSubmit",
        "preToolUse" => "PreToolUse",
        "postToolUse" => "PostToolUse",
        "postToolUseFailure" => "PostToolUseFailure",
        "stop" => "Stop",
        "sessionEnd" => "SessionEnd",
        _ => return provider_event,
    }
    .to_string()
}

/// Extract user prompt text from a UserPromptSubmit raw_json.
/// Tries keys in order: "prompt", "message", "user_message".
/// Truncates to 60 chars (by char count) and appends "…" if longer.
pub fn extract_prompt_text(raw_json: &Value) -> Option<String> {
    let text = ["prompt", "message", "user_message"]
        .iter()
        .find_map(|key| string_field(raw_json, key))?;
    if text.chars().count() <= 60 {
        Some(text)
    } else {
        let truncated: String = text.chars().take(60).collect();
        Some(format!("{truncated}…"))
    }
}

fn string_field(raw: &Value, field: &str) -> Option<String> {
    raw.get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Agent 启动事件：Claude、Codex、Cursor、OpenCode 都必须归一化为 Ready。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   provider 数 = 4；每个 provider 各构造 1 个启动事件；
    ///   Claude/Codex/OpenCode 使用 `SessionStart`，Cursor 使用原生 `sessionStart`；
    ///   node_id 依次为 1、2、3、4，避免不同 provider 的事件互相混淆。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 分别解析四种 provider 的原生启动事件；
    ///   2. Cursor 的小写事件名被归一化为 `SessionStart`；
    ///   3. 将四个标准事件分别映射为 Agent task state。
    ///
    /// 预期结果：
    ///   - 正断言：四种 provider 都分别得到 Ready；
    ///   - 负断言：任何启动事件都不应误映射为 Thinking。
    #[test]
    fn session_start_from_every_agent_provider_maps_to_ready() {
        let claude = parse_hook_event(
            HookSource::Claude,
            1,
            json!({"hook_event_name": "SessionStart", "session_id": "claude-1"}),
        )
        .expect("Claude SessionStart 应可解析");
        let codex = parse_hook_event(
            HookSource::Codex,
            2,
            json!({"hook_event_name": "SessionStart", "session_id": "codex-2"}),
        )
        .expect("Codex SessionStart 应可解析");
        let cursor = parse_hook_event(
            HookSource::Cursor,
            3,
            json!({"hook_event_name": "sessionStart", "conversation_id": "cursor-3"}),
        )
        .expect("Cursor sessionStart 应可解析");
        let opencode = parse_hook_event(
            HookSource::OpenCode,
            4,
            json!({"hook_event_name": "SessionStart", "session_id": "opencode-4"}),
        )
        .expect("OpenCode SessionStart 应可解析");

        assert_eq!(
            task_state_for_provider_event(&claude),
            Some(AgentHookTaskState::Ready),
            "Claude SessionStart 应映射为 Ready"
        );
        assert_eq!(
            task_state_for_provider_event(&codex),
            Some(AgentHookTaskState::Ready),
            "Codex SessionStart 应映射为 Ready"
        );
        assert_eq!(
            task_state_for_provider_event(&cursor),
            Some(AgentHookTaskState::Ready),
            "Cursor sessionStart 应归一化并映射为 Ready"
        );
        assert_eq!(
            task_state_for_provider_event(&opencode),
            Some(AgentHookTaskState::Ready),
            "OpenCode SessionStart 应映射为 Ready"
        );
        assert_ne!(
            task_state_for_provider_event(&claude),
            Some(AgentHookTaskState::Thinking),
            "Claude 启动事件不得误映射为 Thinking"
        );
        assert_ne!(
            task_state_for_provider_event(&codex),
            Some(AgentHookTaskState::Thinking),
            "Codex 启动事件不得误映射为 Thinking"
        );
        assert_ne!(
            task_state_for_provider_event(&cursor),
            Some(AgentHookTaskState::Thinking),
            "Cursor 启动事件不得误映射为 Thinking"
        );
        assert_ne!(
            task_state_for_provider_event(&opencode),
            Some(AgentHookTaskState::Thinking),
            "OpenCode 启动事件不得误映射为 Thinking"
        );
    }

    /// extract_prompt_text: UserPromptSubmit raw_json 中有 "prompt" 字段时返回截断后的文本。
    ///
    /// 数据构造：
    ///   raw_json = {"prompt": "帮我重构 user_service.rs 的错误处理逻辑"}（26字，< 60）
    ///   raw_json_long: prompt 为 70 字的字符串
    ///
    /// 执行过程：
    ///   1. 调用 extract_prompt_text(&raw_json)
    ///   2. 调用 extract_prompt_text(&raw_json_long)
    ///
    /// 预期结果：
    ///   - 正断言：短 prompt 原样返回
    ///   - 正断言：长 prompt 截断到 60 字 + "…"
    ///   - 负断言：长 prompt 不应返回完整 70 字
    #[test]
    fn test_extract_prompt_text_short_and_truncated() {
        let short =
            json!({"hook_event_name": "UserPromptSubmit", "prompt": "帮我重构 user_service.rs"});
        let long_text = "A".repeat(70);
        let long = json!({"hook_event_name": "UserPromptSubmit", "prompt": long_text});

        let result_short = extract_prompt_text(&short);
        let result_long = extract_prompt_text(&long);

        assert_eq!(
            result_short,
            Some("帮我重构 user_service.rs".to_string()),
            "短 prompt 应原样返回"
        );
        let long_result = result_long.expect("长 prompt 应返回 Some");
        assert!(
            long_result.ends_with('…'),
            "长 prompt 应以 … 结尾，实际：{long_result}"
        );
        assert!(
            long_result.chars().count() <= 61,
            "截断后长度应 ≤ 61（60字 + …），实际：{}",
            long_result.chars().count()
        );
        assert_ne!(
            long_result.chars().count(),
            70,
            "长 prompt 不应原样返回完整 70 字"
        );
    }

    /// extract_prompt_text: "message" 和 "user_message" 字段作为备选键；无任何已知键时返回 None。
    ///
    /// 数据构造：
    ///   with_message = {"message": "hello"}
    ///   with_user_message = {"user_message": "hey"}
    ///   no_keys = {}（无任何已知 prompt 键）
    ///
    /// 执行过程：
    ///   1. 调用 extract_prompt_text(&with_message) → 应返回 Some("hello")
    ///   2. 调用 extract_prompt_text(&with_user_message) → 应返回 Some("hey")
    ///   3. 调用 extract_prompt_text(&no_keys) → 应返回 None
    ///
    /// 预期结果：
    ///   - 正断言：有 "message" 时返回 Some("hello")
    ///   - 正断言：有 "user_message" 时返回 Some("hey")
    ///   - 正断言：无任何已知键时返回 None
    #[test]
    fn test_extract_prompt_text_fallback_keys_and_none() {
        let with_message = json!({"hook_event_name": "UserPromptSubmit", "message": "hello"});
        let with_user_message =
            json!({"hook_event_name": "UserPromptSubmit", "user_message": "hey"});
        let no_keys = json!({"hook_event_name": "UserPromptSubmit"});

        assert_eq!(
            extract_prompt_text(&with_message),
            Some("hello".to_string()),
            "有 message 字段时应返回其值"
        );
        assert_eq!(
            extract_prompt_text(&with_user_message),
            Some("hey".to_string()),
            "有 user_message 字段时应返回其值"
        );
        assert_eq!(
            extract_prompt_text(&no_keys),
            None,
            "无已知 prompt 键时应返回 None"
        );
    }
}
