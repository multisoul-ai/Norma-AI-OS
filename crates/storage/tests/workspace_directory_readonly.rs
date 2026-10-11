use agent::workspace_dispatch::*;
use canvas::{AgentProfile, AgentSpec, CanvasRect, Document, NodeKind};
use storage::{
    DatabasePaths, DesktopWorkspaceStore, ProjectStore, WorkspaceAgentRoleStore,
    WorkspaceDirectoryStore,
};

struct Fixture {
    _temp: tempfile::TempDir,
    paths: DatabasePaths,
    workspace: String,
    areas: Vec<storage::DesktopProjectArea>,
    cards: Vec<WorkspaceCardRef>,
}
fn fixture() -> Fixture {
    let temp = tempfile::tempdir().expect("隔离目录应创建");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("项目目录应创建");
    let paths = DatabasePaths::isolated_test(&temp.path().join("data"));
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace = catalog.create("Workspace").expect("应创建空间").id;
    let mut areas = vec![];
    let mut cards = vec![];
    for name in ["Alan", "Bella"] {
        let area = catalog
            .add_project(&workspace, &root)
            .expect("应创建同项目独立区域");
        let mut document = Document::new();
        let node_id = document.add_node(
            NodeKind::Agent(AgentSpec {
                profile: AgentProfile::Codex,
                alias: Some(name.into()),
                definition_id: None,
                mobile_control: false,
            }),
            CanvasRect::new(0., 0., 80., 24.),
        );
        let card = WorkspaceCardRef {
            workspace_id: workspace.clone(),
            area_id: area.id.clone(),
            node_id,
            card_instance_id: document
                .node(node_id)
                .expect("卡片应存在")
                .card_instance_id
                .clone(),
        };
        ProjectStore::initialize_for_desktop_area(&paths, &root, &workspace, &area.id)
            .expect("应初始化区域存储")
            .canvas_store()
            .save_snapshot(&document.to_snapshot())
            .expect("应保存无定义卡片");
        WorkspaceAgentRoleStore::new(paths.app_db_path().to_owned())
            .create(&card, name, true)
            .expect("应登记唯一角色");
        areas.push(area);
        cards.push(card);
    }
    Fixture {
        _temp: temp,
        paths,
        workspace,
        areas,
        cards,
    }
}

// 场景：同一目录两张未加载画布，各有无definition卡且node_id相同；只读完整目录；
// 预期合并为一个项目、保留两张独立角色，不开启Runtime或伪造live指纹。
#[test]
fn readonly_catalog_covers_cold_undefined_cards_across_areas() {
    let f = fixture();
    let before = std::fs::read(f.paths.app_db_path()).expect("应读取查询前DB");
    let directory = WorkspaceDirectoryStore::new(f.paths.app_db_path().to_owned())
        .read(&f.workspace, "desktop", None)
        .expect("完整只读目录应成功");
    assert!(directory.is_complete(), "每个授权区域都已精确匹配登记");
    assert!(
        directory.is_consistent(),
        "完整目录不得包含无效键或跨空间引用"
    );
    assert_eq!(
        directory.projects.len(),
        1,
        "相同project_key应合并跨区域候选"
    );
    assert_eq!(directory.roles.len(), 2, "后台/未加载/无定义卡也必须包含");
    assert_ne!(
        directory.roles[0].card.card_instance_id, directory.roles[1].card.card_instance_id,
        "相同局部node_id不得合并实例"
    );
    assert!(
        directory.roles.iter().all(|r| r.execution.is_none()),
        "存储读取不得编造或启动Runtime"
    );
    assert_eq!(
        std::fs::read(f.paths.app_db_path()).expect("应读取查询后DB"),
        before,
        "目录查询不得迁移或写入数据库"
    );
}

// 场景：手机仅有一个区域授权且其他区域存在有效/旧名称；先过滤再读取；
// 预期不暴露未授权名称、角色、键、区域计数，完整性只针对授权范围。
#[test]
fn restricted_catalog_filters_before_counts_and_retired_keys() {
    let f = fixture();
    WorkspaceAgentRoleStore::new(f.paths.app_db_path().to_owned())
        .rename(&f.cards[1], "Simon")
        .expect("应改名未授权角色");
    let allowed = vec![f.areas[0].id.clone()];
    let directory = WorkspaceDirectoryStore::new(f.paths.app_db_path().to_owned())
        .read(&f.workspace, "grant-1", Some(&allowed))
        .expect("授权只读目录应成功");
    assert_eq!(directory.authorized_area_ids, allowed, "只能返回授权范围");
    assert_eq!(directory.roles.len(), 1, "角色数不得泄漏无权区域");
    assert_eq!(directory.roles[0].name, "Alan", "应只投影授权角色");
    assert!(
        directory.call_keys.iter().all(|k| k.card == f.cards[0]),
        "未授权旧称呼也不得泄漏"
    );
    assert_eq!(
        directory.projects[0].area_ids.len(),
        1,
        "项目范围不包含无权画布数量"
    );
}

// 场景：缺数据库或缺schema，读取目录；构造不存在路径及只含哨兵表的SQLite；
// 预期返回缺口且不建文件、不迁移哨兵数据库。
#[test]
fn missing_database_and_schema_are_never_created_or_migrated() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let path = temp.path().join("missing.sqlite");
    let directory = WorkspaceDirectoryStore::new(path.clone())
        .read("space", "desktop", None)
        .expect("缺文件应返回不完整目录");
    assert!(!directory.is_complete(), "不存在DB不能声称完整");
    assert!(!path.exists(), "严格只读不创建数据库");
    rusqlite::Connection::open(&path)
        .expect("应创建哨兵DB")
        .execute("CREATE TABLE sentinel(value TEXT)", [])
        .expect("应创建哨兵表");
    let before = std::fs::read(&path).expect("应保存哨兵字节");
    let directory = WorkspaceDirectoryStore::new(path.clone())
        .read("space", "desktop", None)
        .expect("缺schema应返回不完整目录");
    assert!(!directory.registration_complete, "schema缺失应明确登记缺口");
    assert_eq!(
        std::fs::read(path).expect("应重读哨兵DB"),
        before,
        "不能使用ensure/migrate读取目录"
    );
}

// 场景：已登记卡片的区域快照被删除，或旧快照缺持久身份；只读目录；
// 预期该区域列为缺口，不把缺文件当空区或后台修复身份。
#[test]
fn missing_or_legacy_snapshot_is_a_coverage_gap() {
    let f = fixture();
    let store = ProjectStore::initialize_for_desktop_area(
        &f.paths,
        &f.areas[0].root,
        &f.workspace,
        &f.areas[0].id,
    )
    .expect("应打开首区域");
    let mut legacy = store
        .canvas_store()
        .load_snapshot()
        .expect("应读取首快照")
        .expect("首快照应存在");
    legacy.nodes[0].card_instance_id.clear();
    store
        .canvas_store()
        .save_snapshot(&legacy)
        .expect("应构造历史缺身份快照");
    let directory = WorkspaceDirectoryStore::new(f.paths.app_db_path().to_owned())
        .read(&f.workspace, "desktop", None)
        .expect("部分目录应返回");
    assert!(
        !directory.is_complete(),
        "缺身份卡片不能用于唯一候选自动选择"
    );
    assert!(
        directory.missing_area_ids.contains(&f.areas[0].id),
        "必须回显实际覆盖缺口"
    );
    assert!(
        store
            .canvas_store()
            .load_snapshot()
            .expect("应重读首区域")
            .expect("首区域应保留")
            .nodes[0]
            .card_instance_id
            .is_empty(),
        "严格只读不得回填历史身份"
    );
    assert_eq!(
        directory.roles.len(),
        1,
        "有效其他区域仍可作为明确标注的部分事实"
    );
}

// 场景：角色revision不变，但项目/画布显示名或area目录发生变化；只读前后目录；
// 预期独立catalog fingerprint变化，新增缺快照区域明确缺口，不能靠role revision跳过复核。
#[test]
fn catalog_version_changes_without_role_revision_and_missing_area_is_not_empty() {
    let f = fixture();
    let reader = WorkspaceDirectoryStore::new(f.paths.app_db_path().to_owned());
    let first = reader
        .read(&f.workspace, "desktop", None)
        .expect("首个目录应读取");
    let catalog = DesktopWorkspaceStore::new(f.paths.app_db_path().to_owned());
    catalog
        .rename_project(&f.workspace, &f.areas[0].id, "Renamed")
        .expect("应改区域显示名");
    let renamed = reader
        .read(&f.workspace, "desktop", None)
        .expect("改名目录应读取");
    assert_eq!(
        renamed.revision, first.revision,
        "catalog变更不会自动改变role revision，正好构成反例"
    );
    assert_ne!(
        renamed.catalog_fingerprint, first.catalog_fingerprint,
        "项目/画布名字变化必须改变catalog指纹"
    );
    let third = catalog
        .add_project(&f.workspace, &f.areas[0].root)
        .expect("应添加无快照区域");
    let added = reader
        .read(&f.workspace, "desktop", None)
        .expect("新增目录应部分返回");
    assert_ne!(
        added.catalog_fingerprint, renamed.catalog_fingerprint,
        "area新增必须改变catalog指纹"
    );
    assert!(
        added.missing_area_ids.contains(&third.id),
        "不存在canvas快照不能当已知空区域"
    );
    assert!(
        !added.is_complete(),
        "新增区域无快照时唯一候选自动选择必须暂缓"
    );
}

// 场景：已登记卡片的Canvas文件彻底缺失；保存其真实路径后删除文件，读取目录；
// 预期名字不被释放，该区域明确缺口，其他区域仍保留部分事实。
#[test]
fn missing_canvas_file_keeps_registration_and_reports_gap() {
    let f = fixture();
    let store = ProjectStore::initialize_for_desktop_area(
        &f.paths,
        &f.areas[0].root,
        &f.workspace,
        &f.areas[0].id,
    )
    .expect("应打开首区域");
    std::fs::remove_file(store.path()).expect("应构造缺canvas文件反例");
    let directory = WorkspaceDirectoryStore::new(f.paths.app_db_path().to_owned())
        .read(&f.workspace, "desktop", None)
        .expect("缺文件应返回部分目录");
    assert!(!directory.is_complete(), "缺文件不能表示该区域空闲或完成");
    assert!(
        directory.missing_area_ids.contains(&f.areas[0].id),
        "必须注明缺失区域"
    );
    assert_eq!(directory.roles.len(), 1, "可读区域仍保留明确部分事实");
    assert!(
        WorkspaceAgentRoleStore::new(f.paths.app_db_path().to_owned())
            .get(&f.cards[0])
            .expect("应只读原登记")
            .is_some(),
        "目录查询不能释放缺文件角色名称"
    );
}

// 场景：未加载画布的一张卡关联已停用配置，另一张是普通无定义有效卡；
// 构造真实共享定义数据库并只读目录，再把关联定义设为未知；预期停用卡仍占名可见，
// 但不成为有效候选，缺启停证据明确coverage gap而不迁移/猜测。
#[test]
fn disabled_defined_card_is_visible_and_missing_definition_is_a_gap() {
    let f = fixture();
    let definitions =
        ProjectStore::initialize_for_desktop_definitions(&f.paths, &f.areas[0].project_key)
            .expect("应初始化共享配置数据库")
            .agent_definition_store();
    let definition = definitions
        .create(&storage::NewAgentDefinition {
            name: "Configured".into(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "Review".into(),
        })
        .expect("应创建配置");
    definitions
        .update(&storage::AgentDefinitionUpdate {
            id: definition.id,
            name: definition.name.clone(),
            runtime_profile: definition.runtime_profile,
            workflow_description: definition.workflow_description.clone(),
            enabled: false,
            sort_order: definition.sort_order,
        })
        .expect("应停用配置");
    let area = ProjectStore::initialize_for_desktop_area(
        &f.paths,
        &f.areas[0].root,
        &f.workspace,
        &f.areas[0].id,
    )
    .expect("应打开目标画布");
    let mut snapshot = area
        .canvas_store()
        .load_snapshot()
        .expect("应读取快照")
        .expect("目标快照存在");
    let NodeKind::Agent(spec) = &mut snapshot.nodes[0].kind else {
        panic!("应构造Agent卡");
    };
    spec.definition_id = Some(definition.id);
    area.canvas_store()
        .save_snapshot(&snapshot)
        .expect("应保存配置关联");
    let reader = WorkspaceDirectoryStore::new(f.paths.app_db_path().to_owned());
    let directory = reader
        .read(&f.workspace, "desktop", None)
        .expect("完整只读目录应成功");
    assert!(directory.is_complete(), "可核对已停用配置不造成未知缺口");
    assert_eq!(
        directory.roles.len(),
        2,
        "停用与普通卡都须保留在空间事实目录"
    );
    assert!(
        !directory
            .roles
            .iter()
            .find(|r| r.card == f.cards[0])
            .expect("停用卡应可见")
            .enabled,
        "停用配置不能成为有效候选"
    );
    assert!(
        directory
            .roles
            .iter()
            .find(|r| r.card == f.cards[1])
            .expect("普通卡应可见")
            .enabled,
        "普通无定义卡仍是有效候选"
    );
    assert!(
        directory.call_keys.iter().any(|key| key.card == f.cards[0]
            && key.key == "alan"
            && key.state == WorkspaceCallKeyStatus::Active),
        "停用卡不释放有效名称占用"
    );
    let NodeKind::Agent(spec) = &mut snapshot.nodes[0].kind else {
        panic!("原卡仍应为Agent");
    };
    spec.definition_id = Some(definition.id + 1000);
    area.canvas_store()
        .save_snapshot(&snapshot)
        .expect("应构造缺配置启停证据");
    let missing = reader
        .read(&f.workspace, "desktop", None)
        .expect("缺证据仍返回部分目录");
    assert!(!missing.is_complete(), "不能猜不存在配置的启用状态");
    assert!(
        missing.missing_area_ids.contains(&f.areas[0].id),
        "未知配置所属区域必须列缺口"
    );
    assert_eq!(missing.roles.len(), 1, "其他区域事实仍保留");
}
