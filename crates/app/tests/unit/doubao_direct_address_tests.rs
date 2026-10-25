use super::plan;
use agent::{
    AgentRosterEntry, AgentTaskState, AgentTrackingMode, DesktopContext, DirectCall, DirectPlan,
};
use norma::protocol::AgentProfile;
use serde_json::json;

fn entry(id: i64, name: &str, alias: Option<&str>, active: bool) -> AgentRosterEntry {
    AgentRosterEntry {
        definition_id: id,
        name: name.to_string(),
        speech_alias: alias.map(str::to_string),
        profile: AgentProfile::Codex,
        enabled: true,
        active_node_id: active.then_some(10 + id as u64),
        tracking: AgentTrackingMode::Managed,
        busy: false,
        task_state: AgentTaskState::Ready,
    }
}

fn roster(alan_active: bool) -> Vec<AgentRosterEntry> {
    vec![
        entry(1, "Alan", Some("阿兰"), alan_active),
        entry(2, "Bella", None, false),
    ]
}

fn call(tool: &'static str, key: &str, text: &str) -> DirectPlan {
    DirectPlan::Calls(vec![DirectCall {
        tool,
        arguments: json!({"agent": "Alan", key: text}),
    }])
}

/// 豆包转写以卡片名开头，且卡片未激活：由代码派发新任务，不交给模型。
///
/// 数据构造：名册含未激活的 Alan 与 Bella；转写“Alan，修复登录页的 bug”以 Alan 开头，
///   名字后的“，”是合法分隔符，正文自包含。
/// 预期结果：恰好一次 assign_agent_task，参数 agent=Alan、task 为去掉称呼后的正文。
#[test]
fn card_name_assigns_task_in_code() {
    assert_eq!(
        plan("Alan，修复登录页的 bug", &roster(false), None),
        call("assign_agent_task", "task", "修复登录页的 bug"),
        "卡片名开头的自包含请求必须由代码派发到该 Agent"
    );
}

/// 语音别名与卡片名等价，已激活卡片走会话回复而不是新任务。
///
/// 数据构造：Alan 已激活（active_node_id=Some），别名“阿兰”；转写“阿兰，继续刚才的方案”。
/// 预期结果：一次 reply_agent，agent 仍解析为规范名 Alan，message 为正文。
#[test]
fn speech_alias_resolves_to_canonical_name_and_replies() {
    assert_eq!(
        plan("阿兰，继续刚才的方案", &roster(true), None),
        call("reply_agent", "message", "继续刚才的方案"),
        "别名必须映射到规范名，已激活卡片应续接会话"
    );
}

/// 名字后没有分隔符时不能当作直呼，避免 “Alanine” 被误派给 Alan。
///
/// 数据构造：转写“Alanine 的配置怎么写”，名册含 Alan。
/// 预期结果：Skip，交给模型处理，零副作用。
#[test]
fn name_without_separator_is_not_matched() {
    assert_eq!(
        plan("Alanine 的配置怎么写", &roster(false), None),
        DirectPlan::Skip,
        "名字后无分隔符不得匹配"
    );
}

/// 没有以 Agent 名开头的普通对话回落到模型。
///
/// 数据构造：“今天天气怎么样”与“帮我看看 Alan 做到哪了”，名称不在开头。
/// 预期结果：两条都是 Skip。
#[test]
fn unmatched_transcript_falls_back_to_model() {
    for text in ["今天天气怎么样", "帮我看看 Alan 做到哪了"] {
        assert_eq!(
            plan(text, &roster(false), None),
            DirectPlan::Skip,
            "未命中应回落到模型：{text}"
        );
    }
}

/// 桌面工作空间使用跨项目工具，直呼路由必须让位给模型。
///
/// 数据构造：同一条可命中的转写，desktop 为 WorkspaceOverview。
/// 预期结果：Skip。
#[test]
fn desktop_workspace_never_uses_direct_route() {
    let desktop = DesktopContext::WorkspaceOverview {
        workspace_id: "w".into(),
        workspace_name: "W".into(),
    };
    assert_eq!(
        plan("Alan，修复登录页的 bug", &roster(false), Some(&desktop)),
        DirectPlan::Skip,
        "桌面工作空间应保持模型 + 跨项目工具"
    );
}
