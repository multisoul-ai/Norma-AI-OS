use norma::workspace_dispatch::{
    CardExecutionFingerprint, WorkspaceCardRef, WorkspaceCardTarget, WorkspaceTargetError,
};

/// 场景：入口无关目标能完整 serde 往返，但旧的定义／名称地址不能冒充持久卡片身份。
/// 数据构造：workspace/area/card-a 为三个独立身份；route_generation=0 表示初始路由，未启动卡无会话。
/// 执行过程：序列化并反序列化目标 → 注入旧 definition_id → 删除 card_instance_id → 校验空身份。
/// 预期结果：完整目标保留所有身份；未知字段、缺失持久卡片身份及空身份均被拒绝，不隐式匹配名称。
#[test]
fn entry_independent_target_round_trips_without_promoting_legacy_addresses() {
    let target = WorkspaceCardTarget {
        card: WorkspaceCardRef {
            workspace_id: "workspace".into(),
            area_id: "area".into(),
            node_id: 1,
            card_instance_id: "card-a".into(),
        },
        execution: CardExecutionFingerprint {
            runtime_instance_id: "runtime-a".into(),
            route_generation: 0,
            terminal_session_id: None,
            run_id: None,
            session_id: None,
        },
    };
    let json = serde_json::to_value(&target).expect("入口无关目标应可序列化");
    let restored: WorkspaceCardTarget =
        serde_json::from_value(json.clone()).expect("完整目标应可恢复");
    assert_eq!(restored, target, "serde 往返必须保留卡片和执行代次两组身份");
    assert_eq!(
        restored.validate(),
        Ok(()),
        "初始路由与未启动会话属于有效结构"
    );
    let mut legacy = json.clone();
    legacy["card"]["definition_id"] = serde_json::json!(42);
    assert!(
        serde_json::from_value::<WorkspaceCardTarget>(legacy).is_err(),
        "旧定义地址不能作为新的实例目标协议被接受"
    );
    let mut missing = json;
    missing["card"]
        .as_object_mut()
        .expect("card 应为对象")
        .remove("card_instance_id");
    assert!(
        serde_json::from_value::<WorkspaceCardTarget>(missing).is_err(),
        "协议缺少持久身份时不能通过名称或 node_id 自动升级"
    );
    let mut invalid = target;
    invalid.card.card_instance_id.clear();
    assert_eq!(
        invalid.validate(),
        Err(WorkspaceTargetError::InvalidIdentity),
        "空身份不能形成可执行目标"
    );
}
