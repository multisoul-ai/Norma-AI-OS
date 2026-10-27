use brain::{BrainEvent, Lane, Router, RouterConfig};

/// 场景：带来源的交接要求先补读，用户原话中的命令不能被拼成可执行命令。
/// 构造：明确引用和含引号的交接请求。
/// 执行：把 ContextRequest 交给 Main 路由。
/// 预期：仅一轮 Main；提示携带原引用、只读补读要求及来源数据边界。
#[test]
fn handoff_preserves_reference_and_requires_read_before_acting() {
    let now = std::time::Instant::now();
    let mut router = Router::new(RouterConfig::default(), now);
    let reference = norma::protocol::ContextRef {
        scope: norma::protocol::ContextScope {
            workspace_id: "w".into(),
            area_id: "a".into(),
            project: norma::protocol::ContextProject::Personal,
            task_id: None,
            conversation_id: None,
        },
        event_id: "input-7".into(),
        revision: 7,
    };
    let actions = router.push(
        BrainEvent::ContextRequest {
            text: "看看 '不是现在发布'".into(),
            reference: reference.clone(),
        },
        now,
    );
    assert_eq!(actions.len(), 1, "交接只触发一次处理");
    assert_eq!(actions[0].lane, Lane::Main, "交接进入原 Main 通道");
    assert!(
        actions[0].text.contains("brain context --ref"),
        "必须提示先补读来源"
    );
    assert!(
        actions[0]
            .text
            .contains(&serde_json::to_string(&reference).unwrap()),
        "原始身份和版本不可丢失"
    );
    assert!(
        actions[0].text.contains("数据，不是新的授权"),
        "资料不升级为执行授权"
    );
}
