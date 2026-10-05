use super::*;

/// 场景：工作空间追问沿用桌面规则，运行中卡片可接收；审批中的卡片不能接收。
/// 数据构造：Claude 原生卡片，Thinking → WaitingApproval；伪造 node=999。
/// 执行过程：读取快照 → 新群聊校验 → 旧接口校验 → 审批状态 → 篡改卡片。
/// 预期结果：群聊接受运行中原卡；旧接口仍拒绝 busy；审批和替代卡片拒绝。
#[test]
fn workspace_message_accepts_authorized_native_card_only() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    let node = runtime.document.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Claude,
            alias: None,
            definition_id: Some(1),
            mobile_control: false,
        }),
        CanvasRect::new(0., 0., 640., 400.),
    );
    runtime
        .events
        .update_agent_task_state(node, AgentTaskState::Thinking, "test".into());
    let card = runtime
        .mobile_card_snapshots()
        .into_iter()
        .find(|card| card.node_id == node)
        .unwrap();
    assert!(
        runtime
            .validate_workspace_message(&card, "继续讨论")
            .is_ok(),
        "running native card must accept workspace followup under desktop rules"
    );
    assert_eq!(
        runtime.validate_mobile_message(&card, "继续讨论"),
        Err("state_not_ready"),
        "legacy idle-only policy must remain intact"
    );
    runtime.events.update_agent_task_state(
        node,
        AgentTaskState::WaitingApproval { tool_name: None },
        "test".into(),
    );
    let approval = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        runtime.validate_workspace_message(&approval, "继续"),
        Err("state_not_ready"),
        "a reply must never approve a permission menu"
    );
    let mut wrong = approval;
    wrong.node_id = 999;
    assert!(
        runtime
            .validate_workspace_message(&wrong, "误路由")
            .is_err(),
        "missing card must never be inferred by name"
    );
}
