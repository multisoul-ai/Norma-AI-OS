use canvas::{AgentProfile, AgentSpec, CanvasRect, Document, NodeKind};
use storage::CanvasStore;

fn agent_kind() -> NodeKind {
    NodeKind::Agent(AgentSpec {
        profile: AgentProfile::Codex,
        alias: Some("Alan".into()),
        definition_id: None,
        mobile_control: false,
    })
}

/// 场景：同一卡片保存、重开和改名后保持持久身份，身份不依赖具名定义。
/// 数据构造：无 definition_id 的 Codex 卡片；新文档首个 node_id=1，终端范围为 80×24。
/// 执行过程：添加卡片 → 保存快照 → 重新打开 CanvasStore → 恢复文档 → 修改 alias。
/// 预期结果：原身份非空且恢复、改名后不变；不得退化成名称或局部 node_id。
#[test]
fn card_identity_survives_storage_restore_and_rename() {
    let temp = tempfile::tempdir().expect("应创建隔离存储目录");
    let path = temp.path().join("canvas.sqlite");
    let mut document = Document::new();
    let node = document.add_node(agent_kind(), CanvasRect::new(0.0, 0.0, 80.0, 24.0));
    let identity = document
        .node(node)
        .expect("卡片应存在")
        .card_instance_id
        .clone();
    CanvasStore::new(path.clone())
        .save_snapshot(&document.to_snapshot())
        .expect("应保存带身份的卡片");
    let snapshot = CanvasStore::new(path)
        .load_snapshot()
        .expect("应读取持久快照")
        .expect("快照应存在");
    let mut restored = Document::from_snapshot(snapshot);
    let card = restored.node_mut(node).expect("恢复的卡片应存在");
    let NodeKind::Agent(spec) = &mut card.kind else {
        panic!("恢复的节点应保持 Agent 类型");
    };
    spec.alias = Some("Simon".into());
    assert!(!identity.is_empty(), "新卡片必须立即拥有持久身份");
    assert_eq!(
        card.card_instance_id, identity,
        "恢复与改名必须保留原卡片身份"
    );
    assert_ne!(card.card_instance_id, "Alan", "显示名不能用作持久身份");
    assert_ne!(
        card.card_instance_id, "1",
        "局部 node_id 不能用作全局卡片身份"
    );
}

/// 场景：历史快照缺失身份字段时只回填一次，重新打开存储仍定位原卡。
/// 数据构造：序列化旧版节点 1，只有 Codex profile，没有 card_instance_id 或 definition_id。
/// 执行过程：兼容 serde 保存旧快照 → 普通读取不回填 → 两个后台 Store 依次回填加载 → 比较身份。
/// 预期结果：旧数据仍可读且获得非空身份；第二次加载不重新生成身份、不绑定虚构定义。
#[test]
fn legacy_snapshot_backfills_one_persistent_identity() {
    let temp = tempfile::tempdir().expect("应创建隔离存储目录");
    let path = temp.path().join("canvas.sqlite");
    let legacy: canvas::DocumentSnapshot = serde_json::from_value(serde_json::json!({
        "nodes": [{"id": 1, "kind": {"Agent": {"profile": "Codex"}},
            "bounds": {"origin": {"x": 0.0, "y": 0.0}, "size": {"x": 80.0, "y": 24.0}},
            "z_index": 1}],
        "viewport": {"pan": {"x": 0.0, "y": 0.0}, "zoom": 1.0}
    }))
    .expect("旧快照应兼容缺失身份字段");
    CanvasStore::new(path.clone())
        .save_snapshot(&legacy)
        .expect("应写入旧快照");
    let raw = CanvasStore::new(path.clone())
        .load_snapshot()
        .expect("兼容读取应成功")
        .expect("旧快照应存在");
    assert!(
        raw.nodes[0].card_instance_id.is_empty(),
        "普通兼容读取不能新增身份回填写库"
    );
    let first = CanvasStore::new(path.clone())
        .load_snapshot_with_card_identity()
        .expect("首次加载应成功")
        .expect("旧快照应存在");
    let second = CanvasStore::new(path)
        .load_snapshot_with_card_identity()
        .expect("重开应成功")
        .expect("旧快照应保留");
    assert!(
        !first.nodes[0].card_instance_id.is_empty(),
        "首次读取必须回填可登记的持久身份"
    );
    assert_eq!(
        first.nodes[0].card_instance_id, second.nodes[0].card_instance_id,
        "回填身份必须持久化，重开不得重新生成"
    );
    let NodeKind::Agent(spec) = &second.nodes[0].kind else {
        panic!("旧卡片必须保持 Agent 类型");
    };
    assert_eq!(spec.definition_id, None, "回填卡片身份不能伪造定义关联");
}

/// 场景：同目录两张独立画布使用相同 node_id 时，持久身份仍独立。
/// 数据构造：1 个空间、1 个目录、2 个 area；每张空文档首个 node_id=1，卡片数量=1+1=2。
/// 执行过程：同目录添加两次 → 分别创建/保存一张卡 → 重开两份区域存储 → 核对身份与节点编号。
/// 预期结果：同编号节点保留各自原身份；两区域不得共用同一个 card_instance_id。
#[test]
fn same_directory_and_node_id_in_two_areas_keep_independent_card_identities() {
    use storage::{DatabasePaths, DesktopWorkspaceStore, ProjectStore};
    let temp = tempfile::tempdir().expect("应创建隔离存储目录");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("应创建项目目录");
    let paths = DatabasePaths::isolated_test(&temp.path().join("data"));
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace = catalog.create("Workspace").expect("应创建空间");
    let first = catalog
        .add_project(&workspace.id, &root)
        .expect("应添加第一张画布");
    let second = catalog
        .add_project(&workspace.id, &root)
        .expect("应添加同目录另一画布");
    let mut left = Document::new();
    let mut right = Document::new();
    let left_node = left.add_node(agent_kind(), CanvasRect::new(0.0, 0.0, 80.0, 24.0));
    let right_node = right.add_node(agent_kind(), CanvasRect::new(0.0, 0.0, 80.0, 24.0));
    let first_store =
        ProjectStore::initialize_for_desktop_area(&paths, &root, &workspace.id, &first.id)
            .expect("应初始化首区域");
    let second_store =
        ProjectStore::initialize_for_desktop_area(&paths, &root, &workspace.id, &second.id)
            .expect("应初始化第二区域");
    first_store
        .canvas_store()
        .save_snapshot(&left.to_snapshot())
        .expect("应保存首卡");
    second_store
        .canvas_store()
        .save_snapshot(&right.to_snapshot())
        .expect("应保存第二卡");
    let loaded_left = first_store
        .canvas_store()
        .load_snapshot_with_card_identity()
        .expect("应读取首卡")
        .expect("首卡应存在");
    let loaded_right = second_store
        .canvas_store()
        .load_snapshot_with_card_identity()
        .expect("应读取第二卡")
        .expect("第二卡应存在");
    assert_eq!(left_node, right_node, "反例必须具有相同局部节点编号");
    assert_eq!(
        loaded_left.nodes[0].card_instance_id,
        left.node(left_node).expect("首卡应存在").card_instance_id,
        "首区域必须恢复原身份"
    );
    assert_eq!(
        loaded_right.nodes[0].card_instance_id,
        right
            .node(right_node)
            .expect("第二卡应存在")
            .card_instance_id,
        "第二区域必须恢复原身份"
    );
    assert_ne!(
        loaded_left.nodes[0].card_instance_id, loaded_right.nodes[0].card_instance_id,
        "同目录、同 node_id、同名称的卡片也必须使用独立身份"
    );
}

/// 场景：两个后台读取者同时迁移同一旧快照，不发布两个不同的身份。
/// 数据构造：旧快照含 1 个缺身份卡片；2 个线程在 Barrier 后并发调用后台回填公共 API。
/// 执行过程：保存缺身份快照 → 并发加载并提交回填 → 重开读取已持久快照。
/// 预期结果：两线程和持久记录获得同一非空身份，不因读取顺序生成另一个角色。
#[test]
fn concurrent_background_backfill_publishes_one_identity() {
    let temp = tempfile::tempdir().expect("应创建隔离存储目录");
    let path = temp.path().join("canvas.sqlite");
    let mut document = Document::new();
    document.add_node(agent_kind(), CanvasRect::new(0.0, 0.0, 80.0, 24.0));
    let mut legacy = document.to_snapshot();
    legacy.nodes[0].card_instance_id.clear();
    CanvasStore::new(path.clone())
        .save_snapshot(&legacy)
        .expect("应保存历史缺身份卡片");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let readers: Vec<_> = (0..2)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                CanvasStore::new(path)
                    .load_snapshot_with_card_identity()
                    .expect("并发后台读取应成功")
                    .expect("历史快照应存在")
                    .nodes[0]
                    .card_instance_id
                    .clone()
            })
        })
        .collect();
    let identities: Vec<_> = readers
        .into_iter()
        .map(|reader| reader.join().expect("读取线程应完成"))
        .collect();
    let persisted = CanvasStore::new(path)
        .load_snapshot()
        .expect("应重开持久快照")
        .expect("持久快照应存在");
    assert!(!identities[0].is_empty(), "并发迁移必须生成有效身份");
    assert_eq!(identities[0], identities[1], "两个读取者必须发布同一身份");
    assert_eq!(
        persisted.nodes[0].card_instance_id, identities[0],
        "并发返回身份必须与已经持久化的身份一致"
    );
}
