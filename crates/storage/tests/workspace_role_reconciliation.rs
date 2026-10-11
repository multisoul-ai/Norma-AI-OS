use agent::workspace_dispatch::WorkspaceCardRef;
use storage::{WorkspaceAgentRoleStore, WorkspaceRoleCandidate};

fn card(area: &str, node: u64) -> WorkspaceCardRef {
    WorkspaceCardRef {
        workspace_id: "space".into(),
        area_id: area.into(),
        node_id: node,
        card_instance_id: format!("{area}-{node}"),
    }
}

/// 场景：全部未加载卡片的旧名字重复，输入顺序不能决定保留者。
/// 数据：b/1 和 a/1 都叫 Alan，a/2 无名且停用；三张不同实例。
/// 执行：逆序批量仲裁，再重复同批仲裁并重开Store读取。
/// 预期：固定 a/1 保留 Alan；其他获得合法唯一名字；停用仍登记；重复恢复不改名。
#[test]
fn reconciles_stably_and_preserves_persisted_mapping() {
    let temp = tempfile::tempdir().unwrap();
    let store = WorkspaceAgentRoleStore::new(temp.path().join("app.sqlite"));
    let candidates = vec![
        WorkspaceRoleCandidate {
            card: card("b", 1),
            preferred_name: Some("Alan".into()),
            enabled: true,
        },
        WorkspaceRoleCandidate {
            card: card("a", 2),
            preferred_name: None,
            enabled: false,
        },
        WorkspaceRoleCandidate {
            card: card("a", 1),
            preferred_name: Some("Alan".into()),
            enabled: true,
        },
    ];
    let report = store.reconcile("space", &candidates).unwrap();
    assert_eq!(report.roles.len(), 3, "无名/停用卡同样必须登记");
    assert_eq!(
        store.get(&card("a", 1)).unwrap().unwrap().name,
        "Alan",
        "固定顺序保留一个旧Alan"
    );
    assert_ne!(
        store.get(&card("b", 1)).unwrap().unwrap().name,
        "Alan",
        "另一重名卡必须自动修复"
    );
    assert!(
        !store.get(&card("a", 2)).unwrap().unwrap().enabled,
        "停用状态保留且占名"
    );
    let before = store.snapshot("space").unwrap();
    let repeated = store.reconcile("space", &candidates).unwrap();
    assert!(repeated.renames.is_empty(), "重复恢复不得再次改名或记录");
    assert_eq!(
        store.snapshot("space").unwrap(),
        before,
        "重复恢复不修改revision或映射"
    );
    assert_eq!(before.call_keys.len(), 6, "共享旧键不能重复变成失效占用");
}

/// 场景：异步命名修复期间用户已移动节点并创建另一节点。
/// 数据：持久两张卡，旧修复只携带其中一卡的身份和称呼。
/// 执行：精确alias补丁，并尝试用旧card_instance_id改同node的新卡。
/// 预期：布局/viewport/新增节点保留，错误实例不改名。
#[test]
fn alias_patch_preserves_latest_layout_and_rejects_replacement() {
    let temp = tempfile::tempdir().unwrap();
    let store = storage::CanvasStore::new(temp.path().join("canvas.sqlite"));
    let mut document = canvas::Document::new();
    let id = document.add_node(
        canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
            profile: canvas::AgentProfile::Codex,
            alias: Some("Alan".into()),
            definition_id: None,
            mobile_control: false,
        }),
        canvas::CanvasRect::new(20.0, 30.0, 400.0, 300.0),
    );
    let identity = document.node(id).unwrap().card_instance_id.clone();
    document.viewport.zoom = 0.75;
    document.add_node(
        canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
            profile: canvas::AgentProfile::Claude,
            alias: Some("Bella".into()),
            definition_id: None,
            mobile_control: false,
        }),
        canvas::CanvasRect::new(900.0, 300.0, 400.0, 300.0),
    );
    let before = document.to_snapshot();
    store.save_snapshot(&before).unwrap();
    store
        .patch_agent_aliases(&[
            (id, identity, "Bruno".into()),
            (id, "replaced".into(), "Chloe".into()),
        ])
        .unwrap();
    let after = store.load_snapshot().unwrap().unwrap();
    assert_eq!(after.nodes.len(), 2, "新增节点不得被旧扫描覆盖");
    assert_eq!(after.viewport, before.viewport, "zoom与pan必须保留");
    assert_eq!(
        after.nodes[0].bounds, before.nodes[0].bounds,
        "最新布局保持"
    );
    let canvas::CanvasNodeKind::Agent(spec) = &after.nodes[0].kind else {
        panic!("应为agent");
    };
    assert_eq!(
        spec.alias.as_deref(),
        Some("Bruno"),
        "仅匹配实例的改名成功，旧实例不能改新卡"
    );
}

/// 场景：旧中文主名艾伦占用Alan的语音别名，修复Alan时其未共享的英文旧键仍需失效占用。
/// 数据：a/1名艾伦、b/1名Alan；中文卡固定排序优先。
/// 执行：整组仲裁后解析两种旧键，再试其他角色复用Alan。
/// 预期：艾伦只有效命中保留者；Alan为原英文角色失效键，不能复用或投递。
#[test]
fn repair_retires_unshared_old_keys_without_stealing_shared_alias() {
    let temp = tempfile::tempdir().unwrap();
    let store = WorkspaceAgentRoleStore::new(temp.path().join("app.sqlite"));
    let candidates = vec![
        WorkspaceRoleCandidate {
            card: card("a", 1),
            preferred_name: Some("艾伦".into()),
            enabled: true,
        },
        WorkspaceRoleCandidate {
            card: card("b", 1),
            preferred_name: Some("Alan".into()),
            enabled: true,
        },
    ];
    store.reconcile("space", &candidates).unwrap();
    assert!(
        matches!(store.resolve("space","艾伦").unwrap(),storage::WorkspaceCallResolution::Active(r) if r.card==card("a",1)),
        "共享旧别名不能变成第二份占用"
    );
    assert!(
        matches!(store.resolve("space","Alan").unwrap(),storage::WorkspaceCallResolution::Inactive(r) if r.card==card("b",1)),
        "未共享的旧主名必须失效并保留原归属"
    );
    assert!(
        store.create(&card("c", 1), "Alan", true).is_err(),
        "历次旧称呼不被其他角色复用"
    );
}
