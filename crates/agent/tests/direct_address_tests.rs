use agent::{parse_direct_address, AgentRosterEntry, AgentTrackingMode, DirectAddressIntent};
use norma::protocol::AgentProfile;

fn alan(active: bool) -> AgentRosterEntry {
    AgentRosterEntry {
        definition_id: 1,
        name: "Alan".to_string(),
        speech_alias: Some("阿兰".to_string()),
        profile: AgentProfile::Codex,
        enabled: true,
        active_node_id: active.then_some(11),
        tracking: AgentTrackingMode::Managed,
        busy: false,
    }
}

fn bella(active: bool) -> AgentRosterEntry {
    AgentRosterEntry {
        definition_id: 2,
        name: "Bella".to_string(),
        speech_alias: None,
        profile: AgentProfile::Claude,
        enabled: true,
        active_node_id: active.then_some(22),
        tracking: AgentTrackingMode::Managed,
        busy: false,
    }
}

/// 活跃 Agent 的句首点名默认进入会话回复，而不是新任务。
#[test]
fn direct_address_active_agent_routes_to_session_reply_not_new_task() {
    let intent = parse_direct_address("Alan，按照 A 方案执行", &[alan(true)]);
    assert!(
        !matches!(intent, DirectAddressIntent::NewTask { .. }),
        "不得误判为新任务"
    );
    match intent {
        DirectAddressIntent::SessionReply { agents, message } => {
            assert_eq!(agents.len(), 1, "应只匹配 Alan");
            assert_eq!(agents[0].name, "Alan", "目标应为 Alan");
            assert_eq!(message, "按照 A 方案执行", "应剥离称呼后保留正文");
        }
        other => panic!("活跃续聊应路由为 SessionReply，实际：{other:?}"),
    }
}

/// 明确“新任务”措辞优先于默认续聊语义。
#[test]
fn explicit_new_task_phrase_overrides_default_reply_semantics() {
    let intent = parse_direct_address("给 Alan 一个新任务：检查支付模块", &[alan(true)]);
    match intent {
        DirectAddressIntent::NewTask { agents, message } => {
            assert_eq!(agents[0].name, "Alan", "新任务目标应为 Alan");
            assert_eq!(message, "检查支付模块", "应保留任务正文");
        }
        other => panic!("明确新任务应路由为 NewTask，实际：{other:?}"),
    }
}

/// 不活跃 + 上下文依赖请求必须零副作用拒绝。
#[test]
fn inactive_agent_context_dependent_request_is_rejected_with_zero_side_effects() {
    let continue_intent = parse_direct_address("Alan，继续刚才那个", &[alan(false)]);
    let scheme_intent = parse_direct_address("Alan，按 A 方案执行", &[alan(false)]);
    match &continue_intent {
        DirectAddressIntent::RejectContextDependent { reason, .. } => {
            assert!(
                reason.contains("还没有可续接的卡片") || reason.contains("卡片或会话"),
                "拒绝话术应指向无卡/无会话，不得引导新建，实际：{reason}"
            );
        }
        other => panic!("继续刚才那个应拒绝，实际：{other:?}"),
    }
    assert!(
        matches!(
            scheme_intent,
            DirectAddressIntent::RejectContextDependent { .. }
        ),
        "按 A 方案执行在不活跃时应拒绝，实际：{scheme_intent:?}"
    );
}

/// 普通提及不得触发回复或任务。
#[test]
fn ordinary_mention_of_agent_name_is_not_a_reply_or_task() {
    let intent = parse_direct_address("我觉得 Alan 这个名字不错", &[alan(true)]);
    assert_eq!(
        intent,
        DirectAddressIntent::NoSideEffect,
        "普通提及必须无副作用"
    );
}

/// 多 Agent 句首点名应按表达顺序形成回复目标集合。
#[test]
fn multi_agent_leading_address_preserves_order() {
    let intent = parse_direct_address("Alan 和 Bella，都按 A 方案执行", &[alan(true), bella(true)]);
    match intent {
        DirectAddressIntent::SessionReply { agents, message } => {
            assert_eq!(agents.len(), 2, "应匹配两个 Agent");
            assert_eq!(agents[0].name, "Alan", "第一目标为 Alan");
            assert_eq!(agents[1].name, "Bella", "第二目标为 Bella");
            assert!(
                message.contains("按 A 方案执行") || message.contains("都按 A 方案执行"),
                "应保留共享回复正文，实际：{message}"
            );
        }
        other => panic!("多 Agent 续聊应路由为 SessionReply，实际：{other:?}"),
    }
}

/// 状态问答不得发送消息。
#[test]
fn status_question_is_read_only() {
    let intent = parse_direct_address("Alan 刚才建议什么？", &[alan(true)]);
    assert_eq!(
        intent,
        DirectAddressIntent::NoSideEffect,
        "状态问答必须只读"
    );
}

/// 不活跃但自包含请求可自动激活派发。
#[test]
fn inactive_self_contained_request_activates_and_assigns() {
    let intent = parse_direct_address("Alan，检查 README", &[alan(false)]);
    match intent {
        DirectAddressIntent::ActivateAndAssign { agents, message } => {
            assert_eq!(agents[0].name, "Alan");
            assert_eq!(message, "检查 README");
        }
        other => panic!("自包含请求应 ActivateAndAssign，实际：{other:?}"),
    }
}
