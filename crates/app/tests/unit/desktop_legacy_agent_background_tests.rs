use super::*;
use canvas::{AgentProfile, AgentSpec, CanvasNodeKind, CanvasRect};

/// 后台加载的旧 Agent 卡片也必须先获得项目级定义，才能经 Norma 安全派活。
///
/// 数据构造：一个桌面工作空间、一个项目区域，以及一张没有 definition_id 的旧 Codex 卡片。
/// 执行过程：保存区域画布后，仅后台加载项目，不进入该项目视图。
/// 预期结果：加载结果中的卡片绑定新定义，项目级定义目录包含同一 ID；
/// 区域画布不再保留无身份的旧卡片。
#[test]
fn background_desktop_load_migrates_legacy_agent_before_dispatch() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("LegacyProject");
    std::fs::create_dir(&root).expect("应创建项目目录");
    let paths = DatabasePaths::isolated_test(&temp.path().join("data"));
    AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = storage::DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace = catalog.create("Legacy").expect("应创建工作空间");
    let area = catalog
        .add_project(&workspace.id, &root)
        .expect("应添加项目区域");
    let area_store =
        ProjectStore::initialize_for_desktop_area(&paths, &root, &workspace.id, &area.id)
            .expect("应初始化区域画布存储");
    let definitions_store =
        ProjectStore::initialize_for_desktop_definitions(&paths, &area.project_key)
            .expect("应初始化项目级定义存储")
            .agent_definition_store();
    let sibling_definition = definitions_store
        .create_quick(&storage::NewAgentDefinition {
            name: "SiblingAgent".into(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: String::new(),
        })
        .expect("应模拟另一张画布正在使用的 Quick Agent 定义");
    let mut runtime = runtime::Runtime::new_for_workspace(root);
    let node_id = runtime.document.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
            alias: Some("OldCodex".into()),
            definition_id: None,
            mobile_control: false,
        }),
        CanvasRect::new(0.0, 0.0, 640.0, 400.0),
    );
    runtime
        .save_canvas_snapshot(&area_store.canvas_store())
        .expect("应保存旧画布卡片");

    let loaded = PreparedProject::load_for_desktop(paths, area).expect("后台加载应成功");
    let mut restored = runtime::Runtime::new();
    restored.restore_canvas_snapshot(loaded.document.expect("应加载区域画布"));
    let definition_id = restored
        .document
        .node(node_id)
        .and_then(|node| match &node.kind {
            CanvasNodeKind::Agent(spec) => spec.definition_id,
            _ => None,
        })
        .expect("旧卡片应在后台加载时完成定义迁移");
    assert!(
        loaded
            .definitions
            .definitions()
            .iter()
            .any(|definition| definition.id == definition_id),
        "迁移后的定义必须进入该项目的 Agent 目录"
    );
    assert!(
        loaded
            .definitions
            .definitions()
            .iter()
            .any(|definition| definition.id == sibling_definition.id),
        "后台加载单一区域不得清理其他区域使用的 Quick Agent 定义"
    );
    assert!(
        !restored.document.nodes().iter().any(|node| {
            matches!(&node.kind, CanvasNodeKind::Agent(spec)
                if spec.definition_id.is_none() && !spec.mobile_control)
        }),
        "后台项目不得继续暴露无定义的普通 Agent 卡片"
    );
}
