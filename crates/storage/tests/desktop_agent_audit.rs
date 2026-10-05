use canvas::AgentProfile;
use storage::{
    AgentDispatchTaskState, AgentDispatchTracking, DatabasePaths, NewAgentDefinition,
    NewOrchestrationAgentDispatchTask, ProjectStore, StoredAgentConversationTail,
    StoredConversationAvailability,
};

fn task(definition: i64, reference: &str) -> NewOrchestrationAgentDispatchTask {
    NewOrchestrationAgentDispatchTask {
        runtime_instance_id: "desktop-a".into(),
        runtime_task_id: 1,
        task_ref: reference.into(),
        root_task_ref: reference.into(),
        parent_task_ref: None,
        source_definition_id: None,
        source_node_id: None,
        source_provider: None,
        target_definition_id: definition,
        target_node_id: 1,
        original_instruction: "保存验收文件".into(),
        task_text: "保存验收文件".into(),
        state: AgentDispatchTaskState::Submitted,
        tracking: AgentDispatchTracking::Managed,
    }
}
fn tail(definition: i64) -> StoredAgentConversationTail {
    StoredAgentConversationTail {
        definition_id: definition,
        node_id: 1,
        session_id: "session-a".into(),
        agent_name: "开发".into(),
        profile: "codex".into(),
        last_user_message: Some("保存验收文件".into()),
        last_agent_message: Some("已完成".into()),
        updated_at_unix: 1,
        availability: StoredConversationAvailability::Available,
        user_message_truncated: false,
        agent_message_truncated: false,
    }
}

/// 场景：项目级 Agent 定义与区域级任务/会话分库后，真实执行审计仍可持久化。
///
/// 数据构造：一个项目、一个共享定义、甲乙两个区域；相同 node_id=1 仅在各自区域内有效。
/// 执行过程：创建共享定义 → 甲写任务与回复 → 重开甲 → 检查乙和甲的本地定义表。
/// 预期结果：甲保留精确任务及回复；乙不含甲任务；不复制共享定义到甲。
#[test]
fn desktop_audit_uses_external_definition_without_duplicating_configuration() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = DatabasePaths::isolated_test(&temp.path().join("data"));
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("项目目录应存在");
    let definitions = ProjectStore::initialize_for_desktop_definitions(&paths, &root)
        .expect("应初始化项目级定义");
    let definition = definitions
        .agent_definition_store()
        .create(&NewAgentDefinition {
            name: "开发".into(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: String::new(),
        })
        .expect("应创建共享定义");
    let alpha = ProjectStore::initialize_for_desktop_area(&paths, &root, "alpha", "area-a")
        .expect("应初始化甲区域");
    let beta = ProjectStore::initialize_for_desktop_area(&paths, &root, "beta", "area-b")
        .expect("应初始化乙区域");
    alpha
        .agent_dispatch_task_store()
        .create_orchestration(&task(definition.id, "desktop-a:1"))
        .expect("区域任务应能引用项目级定义，不能要求本地定义副本");
    alpha
        .agent_conversation_tail_store()
        .upsert(&tail(definition.id))
        .expect("区域回复应能引用项目级定义");
    let reloaded = ProjectStore::initialize_for_desktop_area(&paths, &root, "alpha", "area-a")
        .expect("重复迁移应成功");
    assert_eq!(
        reloaded
            .agent_dispatch_task_store()
            .get_by_task_ref("desktop-a:1")
            .expect("应读取回执")
            .expect("甲任务应存在")
            .node_id,
        1,
        "甲应保留精确卡片身份"
    );
    assert_eq!(
        reloaded
            .agent_conversation_tail_store()
            .get(definition.id, "session-a")
            .expect("应读取回复")
            .expect("甲回复应存在")
            .last_agent_message
            .as_deref(),
        Some("已完成"),
        "原会话回复应持久化"
    );
    assert!(
        beta.agent_dispatch_task_store()
            .get_by_task_ref("desktop-a:1")
            .expect("应读取乙")
            .is_none(),
        "乙不能读取甲任务，即使 node_id 同为 1"
    );
    assert!(
        alpha
            .agent_definition_store()
            .list()
            .expect("应读取本地定义")
            .is_empty(),
        "区域审计不得复制或分叉项目级 Agent 配置"
    );
}

/// 场景：迁移已有桌面区域的旧外键时，保留全部历史任务和会话正文。
///
/// 数据构造：旧单库定义/任务/会话各一条，任务引用 old:1；新共享定义引用取 2=旧 id(1)+1。
/// 执行过程：创建旧格式 → 拷贝至隔离桌面区域 → 官方初始化迁移 → 写外部定义任务。
/// 预期结果：旧任务/回复原文保留，新外部任务能保存；重复初始化不重复数据，普通旧项目仍保留外键。
#[test]
fn desktop_audit_migration_preserves_existing_rows_and_legacy_constraints() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = DatabasePaths::isolated_test(&temp.path().join("data"));
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("项目目录应存在");
    let legacy =
        ProjectStore::initialize_for_database_paths(&paths, &root).expect("旧数据库应初始化");
    let definition = legacy
        .agent_definition_store()
        .create(&NewAgentDefinition {
            name: "开发".into(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: String::new(),
        })
        .expect("应创建旧定义");
    legacy
        .agent_dispatch_task_store()
        .create_orchestration(&task(definition.id, "old:1"))
        .expect("旧任务应保存");
    legacy
        .agent_conversation_tail_store()
        .upsert(&tail(definition.id))
        .expect("旧回复应保存");
    let area = ProjectStore::initialize_for_desktop_area(&paths, &root, "alpha", "area-a")
        .expect("应定位区域数据库");
    std::fs::copy(legacy.path(), area.path()).expect("仅构造隔离旧格式桌面数据库");
    let migrated = ProjectStore::initialize_for_desktop_area(&paths, &root, "alpha", "area-a")
        .expect("已有区域应迁移");
    migrated
        .agent_dispatch_task_store()
        .create_orchestration(&task(definition.id + 1, "desktop-a:1"))
        .expect("迁移后外部定义任务应保存");
    assert_eq!(
        migrated
            .agent_dispatch_task_store()
            .get_by_task_ref("old:1")
            .expect("应读取历史")
            .expect("旧任务必须存在")
            .task_text,
        "保存验收文件",
        "迁移不能丢失原任务正文"
    );
    assert_eq!(
        migrated
            .agent_conversation_tail_store()
            .get(definition.id, "session-a")
            .expect("应读取历史会话")
            .expect("历史回复应存在")
            .last_agent_message
            .as_deref(),
        Some("已完成"),
        "迁移不能丢失会话正文"
    );
    assert!(
        legacy
            .agent_dispatch_task_store()
            .create_orchestration(&task(definition.id + 1, "invalid:1"))
            .is_err(),
        "普通旧项目必须继续校验本地定义外键"
    );
    let again = ProjectStore::initialize_for_desktop_area(&paths, &root, "alpha", "area-a")
        .expect("重复迁移应成功");
    assert_eq!(
        again
            .agent_dispatch_task_store()
            .list()
            .expect("应读取审计")
            .len(),
        2,
        "重复迁移不能复制或删除两条审计记录"
    );
}
