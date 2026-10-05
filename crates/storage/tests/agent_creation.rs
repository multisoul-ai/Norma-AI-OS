use storage::AgentCreationStore;

/// 场景：创建在不同连接重试只获得一次执行权，重启后原卡片绑定仍可查询。
/// 数据构造：设备 phone、Mac mac、空间 a、操作 op；真实定义 3、卡片 7。
/// 执行过程：接收 → 重复接收 → 两次领取 → 绑定身份 → 完成 → 重开查询。
/// 预期结果：第二次领取拒绝；绑定保留；空间 b 看不到回执，篡改请求拒绝。
#[test]
fn creation_claim_and_identity_survive_retry_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.sqlite");
    let store = AgentCreationStore::new(path.clone());
    let payload = r#"{"area_id":"area","profile":"Codex"}"#;
    store.receive("phone", "mac", "a", "op", payload).unwrap();
    store.receive("phone", "mac", "a", "op", payload).unwrap();
    assert!(
        store.claim("phone", "op").unwrap(),
        "首次创建必须取得执行权"
    );
    assert!(!store.claim("phone", "op").unwrap(), "换连接不能重复创建");
    store
        .bind(
            "phone",
            "op",
            &serde_json::json!({"definition_id":3,"node_id":7}),
        )
        .unwrap();
    store.finish("phone", "op", "succeeded", None).unwrap();
    let reopened = AgentCreationStore::new(path);
    let receipt = reopened
        .receipt("phone", "mac", "a", "op")
        .unwrap()
        .unwrap();
    assert_eq!(receipt.state, "succeeded", "重启后必须返回原创建结果");
    assert_eq!(
        receipt.result.unwrap()["node_id"],
        7,
        "不能用另一卡片替换原绑定"
    );
    assert!(
        reopened
            .receipt("phone", "mac", "b", "op")
            .unwrap()
            .is_none(),
        "其它空间不能读取此回执"
    );
    assert!(
        reopened
            .receive("phone", "mac", "b", "op", payload)
            .is_err(),
        "相同操作不能改派空间"
    );
    assert!(
        reopened
            .receive("phone", "mac", "a", "op", r#"{"profile":"Claude"}"#)
            .is_err(),
        "相同操作不能更换类型"
    );
}

/// 场景：Mac 在创建过程中重启，不能重新领取执行，也不能丢掉已有定义绑定。
/// 数据构造：op-1 已领取并绑定 definition 3；op-2 是用户另一次主动创建。
/// 执行过程：创建并绑定 → 重启恢复 → 查询 → 重试领取 → 接收新操作。
/// 预期结果：旧操作结果不明确且仍有绑定；旧操作不可重做，新操作可独立执行。
#[test]
fn interrupted_creation_never_replays_but_new_invocation_can_run() {
    let dir = tempfile::tempdir().unwrap();
    let store = AgentCreationStore::new(dir.path().join("app.sqlite"));
    store.receive("phone", "mac", "a", "op-1", "{}").unwrap();
    store.claim("phone", "op-1").unwrap();
    store
        .bind("phone", "op-1", &serde_json::json!({"definition_id":3}))
        .unwrap();
    store.reconcile_interrupted().unwrap();
    let old = store.receipt("phone", "mac", "a", "op-1").unwrap().unwrap();
    assert_eq!(old.state, "outcome_unknown", "未验明结果不能声称创建成功");
    assert_eq!(
        old.result.unwrap()["definition_id"],
        3,
        "恢复必须保留部分创建身份"
    );
    assert!(
        !store.claim("phone", "op-1").unwrap(),
        "重启后绝不能再次创建原操作"
    );
    store.receive("phone", "mac", "a", "op-2", "{}").unwrap();
    assert!(
        store.claim("phone", "op-2").unwrap(),
        "独立运行快捷指令应创建新的 Agent"
    );
}

/// 场景：重启后的原卡片被核实存在，只更新原操作结果，不能用另一张卡片补回执。
/// 数据构造：op绑定区域area、定义3、node7；错误候选node8，真实候选node7。
/// 执行过程：领取并绑定 → 模拟重启 → 错误身份恢复 → 原身份恢复 → 尝试再领取。
/// 预期结果：错误候选拒绝；原绑定成功；旧操作仍不可再次执行。
#[test]
fn verified_recovery_requires_original_card_identity() {
    let dir = tempfile::tempdir().unwrap();
    let store = AgentCreationStore::new(dir.path().join("app.sqlite"));
    store.receive("phone", "mac", "space", "op", "{}").unwrap();
    store.claim("phone", "op").unwrap();
    store
        .bind(
            "phone",
            "op",
            &serde_json::json!({"area_id":"area","definition_id":3,"node_id":7}),
        )
        .unwrap();
    store.reconcile_interrupted().unwrap();
    let wrong =
        serde_json::json!({"area_id":"area","definition_id":3,"node_id":8,"card":{"node_id":8}});
    assert!(
        store.resolve_unknown("phone", "op", &wrong).is_err(),
        "不能用新卡片8恢复原回执"
    );
    let actual =
        serde_json::json!({"area_id":"area","definition_id":3,"node_id":7,"card":{"node_id":7}});
    store.resolve_unknown("phone", "op", &actual).unwrap();
    let receipt = store
        .receipt("phone", "mac", "space", "op")
        .unwrap()
        .unwrap();
    assert_eq!(receipt.state, "succeeded", "核查到原卡片后应返回成功");
    assert_eq!(
        receipt.result.unwrap()["card"]["node_id"],
        7,
        "真实回执必须保留卡片7"
    );
    assert!(!store.claim("phone", "op").unwrap(), "恢复不应重启一次创建");
}
