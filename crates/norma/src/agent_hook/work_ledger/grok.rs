//! Grok 专有的回合事件判定：子会话过滤、idle_prompt 兜底识别、迟到报告过滤。

use super::{first_string, string_at, NodeLedger};
use crate::agent_hook::AgentHookEvent;
use crate::protocol::AgentProfile;

/// Grok 子会话的生命周期事件属于子 Agent，不是这张卡片的回合；
/// 会话关闭时附带的 Stop（reason 不是 end_turn）也不是回合完成。
pub(super) fn is_grok_main_turn_event(event: &AgentHookEvent) -> bool {
    let raw = &event.raw_json;
    let lifecycle = matches!(
        event.provider_event.as_str(),
        "SessionStart"
            | "SessionEnd"
            | "UserPromptSubmit"
            | "Stop"
            | "StopFailure"
            | "StopCancelled"
    );
    let in_subagent = raw
        .get("subagentType")
        .is_some_and(|value| !value.is_null());
    let session_close_stop = event.provider_event == "Stop"
        && string_at(raw, "reason").is_some_and(|reason| reason != "end_turn");
    !(session_close_stop || (lifecycle && in_subagent))
}

pub(super) fn is_idle_prompt(event: &AgentHookEvent) -> bool {
    event.profile == AgentProfile::Grok
        && first_string(&event.raw_json, &["notificationType", "notification_type"]).as_deref()
            == Some("idle_prompt")
}

/// Grok 的回合结束报告不与下一回合的 UserPromptSubmit 排序；promptId 不同即上一回合的迟到报告。
pub(super) fn is_stale_turn_report(node: Option<&NodeLedger>, event: &AgentHookEvent) -> bool {
    let current = node.and_then(|node| node.prompt_id.as_deref());
    matches!(
        event.provider_event.as_str(),
        "Stop" | "StopFailure" | "StopCancelled"
    ) && current.is_some_and(|current| {
        string_at(&event.raw_json, "promptId").is_some_and(|reported| reported != current)
    })
}
