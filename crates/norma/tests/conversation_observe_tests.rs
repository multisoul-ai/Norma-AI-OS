use norma::agent_hook::{
    observe_conversation, parse_hook_event, ConversationObservationKind,
    ConversationObservationState, HookSource, MAX_CONVERSATION_MESSAGE_CHARS,
};
use serde_json::json;

/// Claude Stop 事件携带最终 assistant 文本时，应产出完整 Agent 回复观察，而非工具输出。
///
/// 数据构造：
///   node_id=11；session_id="claude-s1"；turn_id="t1"
///   last_assistant_message = "方案 A：快速上线；方案 B：先补测试。"
///   同载荷另含 tool_output，验证不得误用工具字段。
///
/// 执行过程：
///   1. parse_hook_event(Claude, …)
///   2. observe_conversation(event)
///
/// 预期结果：
///   - 正断言：kind=CompleteAgentReply，text 为 assistant 正文
///   - 负断言：text 不得等于 tool_output
#[test]
fn claude_stop_event_extracts_complete_agent_reply_not_tool_output() {
    let event = parse_hook_event(
        HookSource::Claude,
        11,
        json!({
            "hook_event_name": "Stop",
            "session_id": "claude-s1",
            "turn_id": "t1",
            "last_assistant_message": "方案 A：快速上线；方案 B：先补测试。",
            "tool_output": "ran tests"
        }),
    )
    .expect("Claude Stop 应可解析");

    let observation = observe_conversation(&event).expect("Stop 应产生会话观察");

    assert_eq!(
        observation.kind,
        ConversationObservationKind::CompleteAgentReply,
        "Stop 且有最终回复时应为 CompleteAgentReply"
    );
    assert_eq!(
        observation.text.as_deref(),
        Some("方案 A：快速上线；方案 B：先补测试。"),
        "应提取 last_assistant_message"
    );
    assert_ne!(
        observation.text.as_deref(),
        Some("ran tests"),
        "不得把 tool_output 当作完整 Agent 回复"
    );
    assert!(
        !observation.truncated,
        "短回复不应标记截断"
    );
}

/// UserPromptSubmit 应保留完整用户消息，并在超过 1,200 Unicode 字符时截断。
///
/// 数值推导：
///   MAX_CONVERSATION_MESSAGE_CHARS = 1_200
///   long_chars = 1_200 + 40 = 1_240
///
/// 预期结果：
///   - 正断言：短消息原样；长消息长度=1200 且 truncated=true
///   - 负断言：长消息不得保留 1240 字
#[test]
fn user_prompt_submit_captures_full_user_message_within_1200_chars() {
    let short = parse_hook_event(
        HookSource::Codex,
        2,
        json!({
            "hook_event_name": "UserPromptSubmit",
            "session_id": "s-short",
            "prompt": "请比较 A/B 方案"
        }),
    )
    .expect("短 prompt 事件应可解析");
    let long_text = "测".repeat(MAX_CONVERSATION_MESSAGE_CHARS + 40);
    let long = parse_hook_event(
        HookSource::Codex,
        2,
        json!({
            "hook_event_name": "UserPromptSubmit",
            "session_id": "s-long",
            "prompt": long_text
        }),
    )
    .expect("长 prompt 事件应可解析");

    let short_obs = observe_conversation(&short).expect("短用户消息应可观察");
    let long_obs = observe_conversation(&long).expect("长用户消息应可观察");

    assert_eq!(
        short_obs.kind,
        ConversationObservationKind::UserMessage,
        "UserPromptSubmit 应为 UserMessage"
    );
    assert_eq!(
        short_obs.text.as_deref(),
        Some("请比较 A/B 方案"),
        "短用户消息应完整保留"
    );
    assert_eq!(
        long_obs.text.as_ref().map(|text| text.chars().count()),
        Some(MAX_CONVERSATION_MESSAGE_CHARS),
        "长用户消息应截断到 1200 字符"
    );
    assert!(
        long_obs.truncated,
        "超预算用户消息应标记 truncated=true"
    );
    assert_ne!(
        long_obs.text.as_ref().map(|text| text.chars().count()),
        Some(MAX_CONVERSATION_MESSAGE_CHARS + 40),
        "不得静默保留超预算全文"
    );
}

/// 重复的 Stop 完成事件不得重复写入会话尾部状态。
///
/// 数据构造：同一 Claude Stop 载荷观察两次。
///
/// 预期结果：
///   - 正断言：第一次 apply=true；第二次 apply=false；last_agent 仍只有一份正文
///   - 负断言：第二次不得覆盖为另一条独立记录
#[test]
fn duplicate_stop_events_are_idempotent_for_conversation_observation() {
    let event = parse_hook_event(
        HookSource::Claude,
        3,
        json!({
            "hook_event_name": "Stop",
            "session_id": "dup-1",
            "turn_id": "turn-9",
            "final_response": "只应保留一次"
        }),
    )
    .expect("重复 Stop fixture 应可解析");
    let first = observe_conversation(&event).expect("第一次观察");
    let second = observe_conversation(&event).expect("第二次观察应仍可生成同指纹事件");

    let mut state = ConversationObservationState::default();
    assert!(
        state.apply(first),
        "首次完整回复观察应被接受"
    );
    assert!(
        !state.apply(second),
        "相同 fingerprint 的重复完成事件必须幂等忽略"
    );
    assert_eq!(
        state.last_agent().and_then(|obs| obs.text.as_deref()),
        Some("只应保留一次"),
        "幂等后仍应保留唯一完整回复"
    );
}

/// 工具输出与无最终正文的 Stop 不得变成 CompleteAgentReply。
///
/// 预期结果：
///   - 正断言：工具载荷 / 空 Stop → UnavailableCompleteAgentReply
///   - 负断言：PreToolUse 不产生会话观察
#[test]
fn tool_output_and_partial_messages_do_not_become_complete_agent_replies() {
    let toolish = parse_hook_event(
        HookSource::Claude,
        4,
        json!({
            "hook_event_name": "Stop",
            "session_id": "tool-1",
            "tool_result": "exit 0"
        }),
    )
    .expect("工具风格 Stop 应可解析");
    let empty_stop = parse_hook_event(
        HookSource::OpenCode,
        5,
        json!({
            "hook_event_name": "Stop",
            "session_id": "idle-1"
        }),
    )
    .expect("空 Stop 应可解析");
    let pre_tool = parse_hook_event(
        HookSource::Codex,
        6,
        json!({
            "hook_event_name": "PreToolUse",
            "session_id": "pre-1",
            "tool_name": "Bash",
            "text": "not a final reply"
        }),
    )
    .expect("PreToolUse 应可解析");

    let tool_obs = observe_conversation(&toolish).expect("工具风格 Stop 仍应有 unavailable 观察");
    let empty_obs = observe_conversation(&empty_stop).expect("空 Stop 应标记 unavailable");

    assert_eq!(
        tool_obs.kind,
        ConversationObservationKind::UnavailableCompleteAgentReply,
        "含 tool_result 的 Stop 不得当作完整回复"
    );
    assert_eq!(
        empty_obs.kind,
        ConversationObservationKind::UnavailableCompleteAgentReply,
        "无正文 Stop 应标记 unavailable"
    );
    assert!(
        observe_conversation(&pre_tool).is_none(),
        "PreToolUse 不得产生完整会话观察"
    );
}

fn assert_provider_stop_contract(source: HookSource, node_id: u64, raw: serde_json::Value) {
    let event = parse_hook_event(source, node_id, raw).expect("provider Stop fixture 应可解析");
    let observation = observe_conversation(&event).expect("Stop 应产生统一契约观察");
    assert_eq!(
        observation.kind,
        ConversationObservationKind::CompleteAgentReply,
        "{source:?} Stop 应映射为 CompleteAgentReply"
    );
    assert_eq!(
        observation.text.as_deref(),
        Some("统一契约回复"),
        "{source:?} 应提取完整回复正文"
    );
    assert!(
        !observation.truncated,
        "{source:?} 短回复不应截断"
    );
}

/// Codex / Cursor / Grok / OpenCode 的固定 Stop 样本应通过同一会话尾部契约。
#[test]
fn provider_stop_events_map_to_same_conversation_contract() {
    assert_provider_stop_contract(
        HookSource::Codex,
        21,
        json!({
            "hook_event_name": "Stop",
            "session_id": "codex-21",
            "final_response": "统一契约回复"
        }),
    );
    assert_provider_stop_contract(
        HookSource::Cursor,
        22,
        json!({
            "hook_event_name": "stop",
            "conversation_id": "cursor-22",
            "lastAssistantMessage": "统一契约回复"
        }),
    );
    assert_provider_stop_contract(
        HookSource::Grok,
        23,
        json!({
            "hookEventName": "Stop",
            "sessionId": "grok-23",
            "assistantMessage": "统一契约回复"
        }),
    );
    assert_provider_stop_contract(
        HookSource::OpenCode,
        24,
        json!({
            "hook_event_name": "Stop",
            "session_id": "opencode-24",
            "response": "统一契约回复"
        }),
    );
}
