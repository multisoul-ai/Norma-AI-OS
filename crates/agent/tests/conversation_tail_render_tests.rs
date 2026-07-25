use agent::{
    render_conversation_tails, AgentConversationAvailability, AgentConversationTail,
    AgentConversationTaskState, ConversationTailRenderHints, MAX_TAIL_BODY_CHARS_TOTAL,
    MAX_TAIL_MESSAGE_CHARS,
};
use norma::protocol::AgentProfile;

fn sample_tail(name: &str, node_id: u64, user: &str, agent: &str, updated_at_unix: i64) -> AgentConversationTail {
    AgentConversationTail {
        definition_id: node_id as i64,
        node_id,
        session_id: format!("session-{node_id}"),
        agent_name: name.to_string(),
        profile: AgentProfile::Codex,
        task_state: AgentConversationTaskState::Ready,
        last_user_message: Some(user.to_string()),
        last_agent_message: Some(agent.to_string()),
        updated_at_unix,
        availability: AgentConversationAvailability::Available,
        user_message_truncated: false,
        agent_message_truncated: false,
    }
}

/// 单条消息超过 1200 字符时必须截断并标记 truncated=true。
///
/// 预算推导：MAX_TAIL_MESSAGE_CHARS = 1200；输入 1201 个 'a'。
/// 预期：输出含 truncated=true，且截断后正文长度为 1200。
#[test]
fn single_message_over_budget_is_truncated_with_marker() {
    let long = "a".repeat(MAX_TAIL_MESSAGE_CHARS + 1);
    let rendered = render_conversation_tails(
        &[sample_tail("Alan", 1, &long, "short", 10)],
        &ConversationTailRenderHints::default(),
    );
    assert!(
        rendered.text.contains("truncated=true"),
        "超长用户消息应标记 truncated=true，实际：{}",
        rendered.text
    );
    assert!(
        !rendered.text.contains(&"a".repeat(MAX_TAIL_MESSAGE_CHARS + 1)),
        "截断后不得保留完整 1201 字符原文"
    );
    assert_eq!(
        rendered.omitted_message_bodies, 0,
        "单条截断不应计为 omitted_message_bodies"
    );
}

/// 总消息体超过 8000 时，后续 Agent 必须保留身份行并省略正文。
///
/// 预算推导：MAX_TAIL_MESSAGE_CHARS=1200，单条截断后 JSON 引号约 +2，
/// 每 Agent 用户+回复约 2404 字符；4 个满额 Agent ≈ 9616 > 8000，
/// 因此至少第 4 个正文应 omitted_due_to_budget。
#[test]
fn total_body_budget_keeps_identity_and_omits_later_bodies() {
    let chunk = "b".repeat(MAX_TAIL_MESSAGE_CHARS);
    let tails = [
        sample_tail("Alan", 1, &chunk, &chunk, 40),
        sample_tail("Bella", 2, &chunk, &chunk, 30),
        sample_tail("Cora", 3, &chunk, &chunk, 20),
        sample_tail("Dana", 4, &chunk, &chunk, 10),
    ];
    let rendered = render_conversation_tails(&tails, &ConversationTailRenderHints::default());
    assert_eq!(
        rendered.included_agents, 4,
        "预算耗尽后仍应包含全部活跃 Agent 身份行"
    );
    assert!(
        rendered.omitted_message_bodies >= 1,
        "至少一个 Agent 的正文应因总预算省略，实际 omitted={}",
        rendered.omitted_message_bodies
    );
    assert!(
        rendered.text.contains("agent=\"Alan\"") && rendered.text.contains("agent=\"Dana\""),
        "应保留首尾 Agent 身份，实际：{}",
        rendered.text
    );
    assert!(
        rendered.text.contains("messages=omitted_due_to_budget"),
        "超预算 Agent 应输出省略标记，实际：{}",
        rendered.text
    );
    assert!(
        rendered
            .text
            .contains(&format!("total_body_budget_{MAX_TAIL_BODY_CHARS_TOTAL}")),
        "省略原因应写明总预算常量"
    );
}

/// named_agents 提示应把点名 Agent 排在最前。
#[test]
fn named_agent_hint_ranks_addressed_agent_first() {
    let tails = [
        sample_tail("Alan", 1, "u1", "a1", 1),
        sample_tail("Bella", 2, "u2", "a2", 100),
    ];
    let rendered = render_conversation_tails(
        &tails,
        &ConversationTailRenderHints {
            named_agents: vec!["Bella".to_string()],
            focused_node_id: None,
            selected_node_ids: Vec::new(),
        },
    );
    let bella_pos = rendered
        .text
        .find("agent=\"Bella\"")
        .expect("应包含 Bella");
    let alan_pos = rendered
        .text
        .find("agent=\"Alan\"")
        .expect("应包含 Alan");
    assert!(
        bella_pos < alan_pos,
        "点名 Bella 应排在 Alan 前，Bella@{bella_pos} Alan@{alan_pos}"
    );
}
