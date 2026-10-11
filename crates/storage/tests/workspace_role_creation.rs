use agent::workspace_dispatch::WorkspaceCardRef;
use canvas::{AgentProfile, AgentSpec, CanvasNodeKind, CanvasNodeSnapshot, CanvasRect};
use storage::WorkspaceAgentRoleStore;

/// 场景：同空间已有Alan时创建新卡，命名成功才持久发布节点。
/// 数据：独立AppDB/CanvasDB，已有角色占Alan；新预留卡同样偏好Alan。
/// 执行：生产两阶段创建service，随后读取持久画布和角色。
/// 预期：得到Bella，canvas和role精确同一身份，原角色不改变；无PTY副作用。
#[test]
fn creation_materializes_unique_role_and_exact_card() {
    let temp = tempfile::tempdir().unwrap();
    let roles = WorkspaceAgentRoleStore::new(temp.path().join("app.sqlite"));
    let existing = WorkspaceCardRef {
        workspace_id: "space".into(),
        area_id: "a".into(),
        node_id: 1,
        card_instance_id: "old".into(),
    };
    roles.create(&existing, "Alan", true).unwrap();
    let card = WorkspaceCardRef {
        node_id: 2,
        card_instance_id: "new".into(),
        ..existing.clone()
    };
    let node = CanvasNodeSnapshot {
        id: 2,
        card_instance_id: "new".into(),
        kind: CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
            alias: Some("Alan".into()),
            definition_id: None,
            mobile_control: false,
        }),
        bounds: CanvasRect::new(0.0, 0.0, 400.0, 300.0),
        z_index: 1,
    };
    let canvas = storage::CanvasStore::new(temp.path().join("canvas.sqlite"));
    let result = roles.materialize_card(&card, &canvas, &node).unwrap();
    assert_eq!(result.name, "Bella", "全空间占用驱动自动分配");
    let persisted = canvas.load_snapshot().unwrap().unwrap();
    assert_eq!(
        persisted.nodes[0].card_instance_id, "new",
        "角色对应原预留卡片"
    );
    let CanvasNodeKind::Agent(spec) = &persisted.nodes[0].kind else {
        panic!("应为Agent");
    };
    assert_eq!(
        spec.alias.as_deref(),
        Some("Bella"),
        "持久标题与有效地址一致"
    );
    assert_eq!(
        roles.get(&existing).unwrap().unwrap().name,
        "Alan",
        "不修改已有角色或共享定义"
    );
    assert!(result.enabled, "持久发布成功后才启用角色");
}

/// 场景：手动指定已占用称呼，或者持久node编号碰撞时，不能启动/创建半张角色。
/// 数据：Alan旧角色，画布node=2已由另一个实例占用。
/// 执行：明确同名创建应冲突；自动创建随后append碰撞并核对不存在的原预留卡。
/// 预期：两种安全失败不改变旧角色，新的名称/身份没有残留。
#[test]
fn explicit_conflict_and_failed_append_do_not_publish_or_hold_names() {
    let temp = tempfile::tempdir().unwrap();
    let roles = WorkspaceAgentRoleStore::new(temp.path().join("app.sqlite"));
    let old = WorkspaceCardRef {
        workspace_id: "space".into(),
        area_id: "a".into(),
        node_id: 1,
        card_instance_id: "old".into(),
    };
    roles.create(&old, "Alan", true).unwrap();
    let card = WorkspaceCardRef {
        node_id: 2,
        card_instance_id: "new".into(),
        ..old.clone()
    };
    let node = CanvasNodeSnapshot {
        id: 2,
        card_instance_id: "new".into(),
        kind: CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
            alias: Some("Alan".into()),
            definition_id: None,
            mobile_control: false,
        }),
        bounds: CanvasRect::new(0.0, 0.0, 400.0, 300.0),
        z_index: 1,
    };
    let canvas = storage::CanvasStore::new(temp.path().join("canvas.sqlite"));
    assert!(
        roles.materialize_named_card(&card, &canvas, &node).is_err(),
        "明确同名不得静默改名"
    );
    assert!(roles.get(&card).unwrap().is_none(), "冲突不能留下半登记");
    let mut existing = node.clone();
    existing.card_instance_id = "other".into();
    canvas.insert_reserved_card(&existing).unwrap();
    assert!(
        roles.materialize_card(&card, &canvas, &node).is_err(),
        "不同实例不能覆盖相同node"
    );
    assert!(
        roles.get(&card).unwrap().is_none(),
        "确知原预留卡不存在后应释放半创建键"
    );
    assert_eq!(
        roles.snapshot("space").unwrap().call_keys.len(),
        2,
        "仅旧Alan两键保留"
    );
}

/// 场景：生产创建等待期间原区域已被删除，旧请求不得创建任何具名卡/预留名称。
/// 数据：真实空间与区域，随后只删除区域catalog；请求仍持有原区域身份。
/// 执行：调用生产materialize_workspace_card而非低层测试写入接口。
/// 预期：明确拒绝原区域，角色/键为零，原画布不新增节点。
#[test]
fn production_materialization_rejects_removed_area_before_reserving_names() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let catalog = storage::DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let space = catalog.create("space").unwrap();
    let area = catalog.add_project(&space.id, &root).unwrap();
    let canvas =
        storage::ProjectStore::initialize_for_desktop_area(&paths, &area.root, &space.id, &area.id)
            .unwrap()
            .canvas_store();
    let roles = WorkspaceAgentRoleStore::new(paths.app_db_path().to_owned());
    let card = WorkspaceCardRef {
        workspace_id: space.id.clone(),
        area_id: area.id.clone(),
        node_id: 1,
        card_instance_id: "reserved".into(),
    };
    let node = CanvasNodeSnapshot {
        id: 1,
        card_instance_id: "reserved".into(),
        kind: CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
            alias: Some("Alan".into()),
            definition_id: None,
            mobile_control: false,
        }),
        bounds: CanvasRect::new(0.0, 0.0, 400.0, 300.0),
        z_index: 1,
    };
    catalog.remove_project_catalog(&space.id, &area.id).unwrap();
    assert!(
        roles
            .materialize_workspace_card(&card, &canvas, &node, false)
            .is_err(),
        "失效区域必须拒绝"
    );
    assert!(
        roles.snapshot(&space.id).unwrap().roles.is_empty(),
        "不得接受或占用新角色名称"
    );
    assert!(
        canvas.load_snapshot().unwrap().is_none(),
        "不得创建原区域卡片"
    );
}
