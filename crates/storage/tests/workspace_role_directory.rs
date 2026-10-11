use canvas::{AgentProfile, AgentSpec, CanvasNodeKind, CanvasRect, Document};
use storage::{DatabasePaths, DesktopWorkspaceStore, ProjectStore, WorkspaceAgentRoleStore};

fn document(name: Option<&str>, definition_id: Option<i64>) -> Document {
    let mut doc = Document::new();
    doc.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::OpenCode,
            alias: name.map(str::to_owned),
            definition_id,
            mobile_control: false,
        }),
        CanvasRect::new(40.0, 50.0, 400.0, 300.0),
    );
    doc
}

/// 场景：完整登记包括同目录的未加载普通卡、无定义卡及停用配置的卡。
/// 数据：三个独立区域，两个Alan旧名、一张无名停用卡；另一空间复用同配置目录。
/// 执行：不建立Runtime/PTY，直接生产register_workspace；重开重复扫描后比较映射。
/// 预期：固定保留一个Alan且其余自动唯一；停用占名；配置/另一空间原快照不修改。
#[test]
fn full_directory_includes_unloaded_plain_and_disabled_cards_without_changing_definitions() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let paths = DatabasePaths::isolated_test(&temp.path().join("data"));
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let space = catalog.create("space").unwrap();
    let other = catalog.create("other").unwrap();
    let definitions = ProjectStore::initialize_for_desktop_definitions(&paths, &root)
        .unwrap()
        .agent_definition_store();
    let disabled = definitions
        .create(&storage::NewAgentDefinition {
            name: "Chloe".into(),
            runtime_profile: AgentProfile::OpenCode,
            workflow_description: String::new(),
        })
        .unwrap();
    definitions
        .update(&storage::AgentDefinitionUpdate {
            id: disabled.id,
            name: "Chloe".into(),
            runtime_profile: AgentProfile::OpenCode,
            workflow_description: String::new(),
            enabled: false,
            sort_order: 0,
        })
        .unwrap();
    let before_definitions = definitions.list().unwrap();
    let areas = (0..3)
        .map(|_| catalog.add_project(&space.id, &root).unwrap())
        .collect::<Vec<_>>();
    let mut stores = Vec::new();
    for (index, area) in areas.iter().enumerate() {
        let project =
            ProjectStore::initialize_for_desktop_area(&paths, &root, &space.id, &area.id).unwrap();
        let doc = document(
            if index < 2 { Some("Alan") } else { None },
            if index == 2 { Some(disabled.id) } else { None },
        );
        let mut legacy = doc.to_snapshot();
        legacy.nodes[0].card_instance_id.clear();
        project.canvas_store().save_snapshot(&legacy).unwrap();
        stores.push(project);
    }
    let other_area = catalog.add_project(&other.id, &root).unwrap();
    let other_store =
        ProjectStore::initialize_for_desktop_area(&paths, &root, &other.id, &other_area.id)
            .unwrap();
    let other_snapshot = document(Some("Alan"), Some(disabled.id)).to_snapshot();
    other_store
        .canvas_store()
        .save_snapshot(&other_snapshot)
        .unwrap();
    let roles = WorkspaceAgentRoleStore::new(paths.app_db_path().to_owned());
    let report = roles.register_workspace(&space.id).unwrap();
    assert_eq!(report.roles.len(), 3, "全部区域普通和无定义卡均参与登记");
    assert_eq!(
        report.roles.iter().filter(|r| r.name == "Alan").count(),
        1,
        "恰有一个稳定旧Alan保留"
    );
    assert_eq!(
        report.roles.iter().filter(|r| !r.enabled).count(),
        1,
        "停用角色仍登记且占名"
    );
    assert_eq!(
        roles.repairs(&space.id).unwrap().len(),
        2,
        "两个自动改名映射有持久记录"
    );
    let before = roles.snapshot(&space.id).unwrap();
    let repeated = roles.register_workspace(&space.id).unwrap();
    assert!(repeated.renames.is_empty(), "重复扫描不得再自动改名");
    assert_eq!(
        roles.snapshot(&space.id).unwrap(),
        before,
        "重复登记不修改名称/revision/停用状态"
    );
    assert_eq!(
        definitions.list().unwrap(),
        before_definitions,
        "不得修改共享定义影响其他空间"
    );
    assert_eq!(
        other_store.canvas_store().load_snapshot().unwrap().unwrap(),
        other_snapshot,
        "另一空间原卡标题和布局完全保留"
    );
    for (area, store) in areas.iter().zip(stores) {
        let snapshot = store.canvas_store().load_snapshot().unwrap().unwrap();
        let node = &snapshot.nodes[0];
        let role = report
            .roles
            .iter()
            .find(|r| r.card.area_id == area.id)
            .unwrap();
        assert_eq!(
            node.card_instance_id, role.card.card_instance_id,
            "回填身份先持久后登记"
        );
        let CanvasNodeKind::Agent(spec) = &node.kind else {
            panic!("应为agent");
        };
        assert_eq!(
            spec.alias.as_deref(),
            Some(role.name.as_str()),
            "持久卡片标题与有效目录名称一致"
        );
    }
}

/// 场景：Runtime关闭或未保存删除不能释放名称，永久删除持久画布后才释放全部历史键。
/// 数据：一张Alan改名Bella（四个当前/历史键）；Runtime内先移除但不保存。
/// 执行：完整扫描保留占用→保存空画布→再次扫描→同名新身份创建。
/// 预期：未持久删除保留四键，持久删除成功清空原角色键，新角色获得不同身份。
#[test]
fn only_acknowledged_persistent_removal_releases_old_and_current_names() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let paths = DatabasePaths::isolated_test(&temp.path().join("data"));
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let space = catalog.create("space").unwrap();
    let area = catalog.add_project(&space.id, &root).unwrap();
    let project =
        ProjectStore::initialize_for_desktop_area(&paths, &area.root, &space.id, &area.id).unwrap();
    let mut doc = document(Some("Alan"), None);
    project
        .canvas_store()
        .save_snapshot(&doc.to_snapshot())
        .unwrap();
    let roles = WorkspaceAgentRoleStore::new(paths.app_db_path().to_owned());
    let first = roles.register_workspace(&space.id).unwrap().roles.remove(0);
    roles.rename(&first.card, "Bella").unwrap();
    doc.remove_node(first.card.node_id);
    roles.register_workspace(&space.id).unwrap();
    assert_eq!(
        roles.snapshot(&space.id).unwrap().call_keys.len(),
        4,
        "Runtime消失不能释放名称"
    );
    project
        .canvas_store()
        .save_snapshot_with_card_removals(
            &doc.to_snapshot(),
            &[(first.card.node_id, first.card.card_instance_id.clone())],
        )
        .unwrap();
    roles.register_workspace(&space.id).unwrap();
    assert!(
        roles.snapshot(&space.id).unwrap().call_keys.is_empty(),
        "持久删除成功才释放全部键"
    );
    let fresh = agent::workspace_dispatch::WorkspaceCardRef {
        card_instance_id: "new-instance".into(),
        ..first.card.clone()
    };
    let created = roles.create(&fresh, "Alan", true).unwrap();
    assert_ne!(
        created.card.card_instance_id, first.card.card_instance_id,
        "名称复用必须属于新身份"
    );
}

/// 场景：区域catalog删除成功但canvas文件删除失败/尚未完成，不得提前级联释放角色。
/// 数据：真实区域中的Alan角色及持久卡片。
/// 执行：只删catalog后检查呼叫键，再完成canvas文件删除与显式释放。
/// 预期：catalog阶段仍占名；只有成功删文件后的确认步骤才释放。
#[test]
fn catalog_removal_alone_never_releases_names() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let paths = DatabasePaths::isolated_test(&temp.path().join("data"));
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let space = catalog.create("space").unwrap();
    let area = catalog.add_project(&space.id, &root).unwrap();
    let project =
        ProjectStore::initialize_for_desktop_area(&paths, &area.root, &space.id, &area.id).unwrap();
    project
        .canvas_store()
        .save_snapshot(&document(Some("Alan"), None).to_snapshot())
        .unwrap();
    let roles = WorkspaceAgentRoleStore::new(paths.app_db_path().to_owned());
    roles.register_workspace(&space.id).unwrap();
    assert!(
        catalog.remove_project_catalog(&space.id, &area.id).unwrap(),
        "catalog删除成功"
    );
    assert_eq!(
        roles.snapshot(&space.id).unwrap().call_keys.len(),
        2,
        "文件删除未成功前继续占名"
    );
    catalog.remove_canvas_file(&space.id, &area.id).unwrap();
    roles
        .delete_after_area_removed(&space.id, &area.id)
        .unwrap();
    assert!(
        roles.snapshot(&space.id).unwrap().call_keys.is_empty(),
        "确认彻底删除后释放"
    );
}

/// 场景：数据文件丢失不等于用户已成功删除角色。
/// 数据：已登记Alan的真实区域，模拟文件丢失但保留catalog。
/// 执行：删文件后重新完整登记。
/// 预期：明确目录缺口，全部名字保留，不把数据缺失当删除成功。
#[test]
fn missing_canvas_without_deletion_proof_retains_role_names() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let paths = DatabasePaths::isolated_test(&temp.path().join("data"));
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let space = catalog.create("space").unwrap();
    let area = catalog.add_project(&space.id, &root).unwrap();
    let project =
        ProjectStore::initialize_for_desktop_area(&paths, &area.root, &space.id, &area.id).unwrap();
    project
        .canvas_store()
        .save_snapshot(&document(Some("Alan"), None).to_snapshot())
        .unwrap();
    let roles = WorkspaceAgentRoleStore::new(paths.app_db_path().to_owned());
    roles.register_workspace(&space.id).unwrap();
    let before = roles.snapshot(&space.id).unwrap();
    catalog.remove_canvas_file(&space.id, &area.id).unwrap();
    assert!(
        roles.register_workspace(&space.id).is_err(),
        "缺文件必须报告缺口，不能假装完整目录"
    );
    assert_eq!(
        roles.snapshot(&space.id).unwrap(),
        before,
        "没有成功删除凭据不得释放有效或历史名称"
    );
}
