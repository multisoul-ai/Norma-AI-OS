use norma::mobile::{WorkspaceChatMessage, WorkspaceMessageIntent};

/// 场景：普通讨论默认不会获得执行意图，卡片目标必须完整且属于当前空间。
/// 数据构造：一个无目标消息和一个缺少 node_id 的目标 JSON；各一条。
/// 执行过程：反序列化讨论 → 检查默认意图 → 尝试解析不完整目标。
/// 预期结果：讨论为 Discuss；不完整卡片目标拒绝，不根据名称补齐。
#[test]
fn discussion_defaults_to_no_dispatch_and_target_is_complete() {
    let message: WorkspaceChatMessage = serde_json::from_str(r#"{"desktop_workspace_id":"space","operation_id":"op","message_id":"message","text":"这个设计怎么样"}"#).unwrap();
    assert_eq!(
        message.intent,
        WorkspaceMessageIntent::Discuss,
        "normal message must not imply execution"
    );
    assert!(
        message.target.is_none(),
        "discussion must not invent an Agent target"
    );
    assert!(serde_json::from_str::<WorkspaceChatMessage>(r#"{"desktop_workspace_id":"space","operation_id":"op","message_id":"message","text":"执行","target":{"area_id":"area","definition_id":1}}"#).is_err(), "partial target must not be resolved by name or current selection");
}

/// 场景：同名 Agent、桌面切换或运行变化不能把手机消息发给其它卡片。
/// 数据构造：目标区域 A、definition 1、node 7、run r/session s；另一张 node 8 同名卡片。
/// 执行过程：匹配真实快照 → 改卡片 → 改 definition → 改 session → 设置原任务但移除会话。
/// 预期结果：原目标通过；其它卡片、定义与会话拒绝；未绑定原任务会话不能追问。
#[test]
fn exact_target_never_follows_name_or_selection() {
    let mut target: norma::mobile::WorkspaceAgentTarget = serde_json::from_value(serde_json::json!({"area_id":"a","definition_id":1,"node_id":7,"run_id":"r","session_id":"s"})).unwrap();
    let mut card: norma::mobile::CardSnapshot = serde_json::from_value(serde_json::json!({"node_id":7,"profile":"Codex","title":"Same","mobile_control":false,"state":"idle","run_id":"r","session_id":"s","task_title":null})).unwrap();
    assert!(
        target.validate_card("a", 1, &card).is_ok(),
        "exact original card must be accepted"
    );
    assert!(
        target.validate_card("b", 1, &card).is_err(),
        "current desktop area must not override bound area"
    );
    card.node_id = 8;
    assert!(
        target.validate_card("a", 1, &card).is_err(),
        "same name on different card must be rejected"
    );
    card.node_id = 7;
    assert!(
        target.validate_card("a", 2, &card).is_err(),
        "changed definition must be rejected"
    );
    card.session_id = Some("new".into());
    assert!(
        target.validate_card("a", 1, &card).is_err(),
        "changed session must not silently replace target"
    );
    target.task_ref = Some("task-1".into());
    target.run_id = None;
    target.session_id = None;
    card.run_id = None;
    card.session_id = None;
    assert!(
        target.validate_card("a", 1, &card).is_err(),
        "followup requires frozen original run and session"
    );
}
