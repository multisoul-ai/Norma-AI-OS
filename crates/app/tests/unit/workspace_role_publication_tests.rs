use super::*;

/// 场景：修改当前角色称呼只能更新原卡片标题，不能重建Runtime或旧任务会话。
/// 数据：两张无定义卡，第一张冻结执行target；伪造第二张同node旧实例。
/// 执行：project_role_title原位改名，再尝试错误实例投影。
/// 预期：目标指纹完全不变，错误实例不能覆盖新名，第二张不改变。
#[test]
fn title_projection_preserves_execution_and_rejects_other_instance() {
    let mut runtime = runtime::Runtime::new();
    let kind = canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
        profile: canvas::AgentProfile::Codex,
        alias: Some("Alan".into()),
        definition_id: None,
        mobile_control: false,
    });
    let node = runtime.document.add_node(
        kind.clone(),
        canvas::CanvasRect::new(0.0, 0.0, 400.0, 300.0),
    );
    runtime
        .document
        .add_node(kind, canvas::CanvasRect::new(500.0, 0.0, 400.0, 300.0));
    let target = runtime
        .workspace_card_target("space", "area", 7, node)
        .unwrap();
    let role = storage::WorkspaceAgentRole {
        card: target.card.clone(),
        name: "Bella".into(),
        speech_alias: Some("贝拉".into()),
        enabled: true,
        created_at_unix: 0,
        updated_at_unix: 0,
    };
    assert!(project_role_title(&mut runtime, &role), "原位改名成功");
    assert_eq!(
        runtime
            .workspace_card_target("space", "area", 7, node)
            .unwrap(),
        target,
        "仅称呼修改不得失效已有任务指纹"
    );
    let mut wrong = role.clone();
    wrong.card.card_instance_id = "replacement".into();
    wrong.name = "Bruno".into();
    assert!(
        !project_role_title(&mut runtime, &wrong),
        "旧实例不得改名新卡"
    );
    let canvas::CanvasNodeKind::Agent(spec) = &runtime.document.nodes()[0].kind else {
        panic!("应为agent");
    };
    assert_eq!(
        spec.alias.as_deref(),
        Some("Bella"),
        "目标标题仍为已保存名称"
    );
    let canvas::CanvasNodeKind::Agent(other) = &runtime.document.nodes()[1].kind else {
        panic!("应为agent");
    };
    assert_eq!(
        other.alias.as_deref(),
        Some("Alan"),
        "不可按definition同步其他卡片"
    );
}

/// 场景：后台已精确追加卡片，前台Document尚未安装，正常dirty writer不能抹除它。
/// 数据：旧空Document、持久一张新卡；publication barrier计数为1。
/// 执行：dirty flush尝试→核对持久卡→安装新卡→释放barrier→flush。
/// 预期：等待期间持久卡存在，结束后最新Document持久成功；无前台阻塞等待。
#[test]
fn pending_publication_blocks_stale_whole_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let project = storage::ProjectStore::new(temp.path().join("canvas.sqlite"));
    let mut scheduler = crate::workspace::persistence::PersistenceScheduler::default();
    scheduler.set_project_store(project.clone());
    let publication = scheduler.begin_card_publication();
    scheduler.mark_canvas_dirty();
    let mut runtime = runtime::Runtime::new();
    let card = runtime.document.reserve_node(
        canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
            profile: canvas::AgentProfile::Codex,
            alias: Some("Alan".into()),
            definition_id: None,
            mobile_control: false,
        }),
        canvas::CanvasRect::new(0.0, 0.0, 400.0, 300.0),
    );
    project.canvas_store().insert_reserved_card(&card).unwrap();
    let layout = crate::canvas::master_layout::MasterLayoutState::default();
    scheduler.flush_now(&runtime, &layout).unwrap();
    assert_eq!(
        project
            .canvas_store()
            .load_snapshot()
            .unwrap()
            .unwrap()
            .nodes
            .len(),
        1,
        "旧Document不得抹除后台已发布卡"
    );
    assert!(
        runtime.document.install_reserved_node(card),
        "前台安装原身份"
    );
    scheduler.end_card_publication(&publication);
    scheduler.flush_now(&runtime, &layout).unwrap();
    assert_eq!(
        scheduler.save_state(),
        crate::workspace::persistence::PersistenceSaveState::Clean,
        "安装完成恢复正常保存"
    );
}

/// 场景：后台持久创建已成功，前台因运行实例过期拒绝安装，旧 Document 仍有脏保存。
/// 数据：空 Runtime、真实 CanvasDB 与 AppDB；创建一张具名卡并持久化其角色。
/// 执行：结束迟到回调但不声称卡片不存在，再触发旧 Document 的 whole snapshot flush。
/// 预期：最终重读 Canvas 仍含原 UUID；角色及名字占用仍存在；原 Runtime 保持空。
#[test]
fn refused_late_publication_preserves_durable_card_and_role() {
    let temp = tempfile::tempdir().unwrap();
    let project = storage::ProjectStore::new(temp.path().join("canvas.sqlite"));
    let roles = storage::WorkspaceAgentRoleStore::new(temp.path().join("app.sqlite"));
    let mut scheduler = crate::workspace::persistence::PersistenceScheduler::default();
    scheduler.set_project_store(project.clone());
    let publication = scheduler.begin_card_publication();
    let mut runtime = runtime::Runtime::new();
    let node = runtime.document.reserve_node(
        canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
            profile: canvas::AgentProfile::Codex,
            alias: Some("Alan".into()),
            definition_id: None,
            mobile_control: false,
        }),
        canvas::CanvasRect::new(0.0, 0.0, 400.0, 300.0),
    );
    let card = WorkspaceCardRef {
        workspace_id: "space".into(),
        area_id: "area".into(),
        node_id: node.id,
        card_instance_id: node.card_instance_id.clone(),
    };
    roles
        .materialize_card(&card, &project.canvas_store(), &node)
        .unwrap();
    // 同一 DB 的新绑定可能在后台 append 之前完成快照读取，先重置 scheduler 模拟它。
    scheduler.set_project_store(project.clone());
    scheduler.finish_card_publication(&publication, false, false);
    let other_publication = scheduler.begin_card_publication();
    scheduler.finish_card_publication(&other_publication, false, true);
    scheduler.mark_canvas_dirty();
    scheduler.flush_now(&runtime, &Default::default()).unwrap();
    let persisted = project.canvas_store().load_snapshot().unwrap().unwrap();
    assert_eq!(
        persisted.nodes.len(),
        1,
        "迟到拒绝不能让旧 snapshot 抹除已发布卡"
    );
    assert_eq!(
        persisted.nodes[0].card_instance_id, card.card_instance_id,
        "必须保留原持久卡 UUID"
    );
    assert!(
        roles.get(&card).unwrap().is_some(),
        "无法安装不等于永久删除，角色必须保留"
    );
    assert!(
        matches!(roles.resolve("space", "Alan").unwrap(), storage::WorkspaceCallResolution::Active(role) if role.card == card),
        "已持久角色名称继续占用，不得补偿释放"
    );
    assert!(
        runtime.document.nodes().is_empty(),
        "拒绝路径不得偷偷安装旧卡片"
    );
    assert_eq!(
        scheduler.save_state(),
        crate::workspace::persistence::PersistenceSaveState::Pending,
        "后续正常回调不能清除未知发布冻结"
    );
}
