use storage::{
    AgentDispatchTaskState, AgentDispatchTracking, DatabasePaths, DesktopWorkspaceStore,
    NewAgentDispatchTask, ProjectStore,
};
/// 场景：桌面画布移除定义外键后仍记录真实状态迁移；无定义的卡片时间为null。
/// 构造：临时工作空间、画布及定义ID=7的任务，queued→cancelled共两条迁移。
/// 执行：创建/更新任务，读ConnectorReadStore；预期：新→旧历史精确，重复状态不产生伪事件。
#[test]
fn connector_records_actual_task_transitions_after_desktop_migration() {
    let dir = tempfile::tempdir().expect("临时库");
    let paths = DatabasePaths::isolated_test(&dir.path().join("data"));
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace = catalog.create("time").expect("工作空间");
    let area = catalog
        .add_project(&workspace.id, dir.path())
        .expect("区域");
    let project =
        ProjectStore::initialize_for_desktop_area(&paths, dir.path(), &workspace.id, &area.id)
            .expect("画布库");
    let tasks = project.agent_dispatch_task_store();
    let task = tasks
        .create(&NewAgentDispatchTask {
            runtime_task_id: 1,
            definition_id: 7,
            node_id: 1,
            original_instruction: "work".into(),
            task_text: "work".into(),
            state: AgentDispatchTaskState::Queued,
            tracking: AgentDispatchTracking::Managed,
        })
        .expect("桌面任务外键正确");
    tasks
        .finish_task(
            task.runtime_task_id,
            AgentDispatchTaskState::Cancelled,
            None,
        )
        .expect("真实迁移");
    tasks
        .finish_task(
            task.runtime_task_id,
            AgentDispatchTaskState::Cancelled,
            None,
        )
        .expect("重复状态");
    let data = storage::connector_read::ConnectorReadStore::new(paths.app_db_path().to_owned())
        .read()
        .expect("读取");
    let task = &data[0].areas[0].tasks[0];
    assert_eq!(task.events.len(), 2, "queued与cancelled两条真实事件");
    assert_eq!(task.events[0].1, "cancelled", "最新在前");
    assert_eq!(task.events[1].1, "queued", "保留初始状态");
    assert!(task.events[0].0 >= task.events[1].0, "时间顺序");
    assert!(
        task.events[0].0 > 1_000_000_000 && task.events[0].0 < 10_000_000_000,
        "unix秒而非毫秒"
    );
}
