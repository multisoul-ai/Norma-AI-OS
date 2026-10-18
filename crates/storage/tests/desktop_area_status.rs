use storage::{DesktopAgentStatus, DesktopAreaStatusStore, DesktopWorkspaceStore};

/// 未加载项目应保留最后一次真实 Agent 状态和更新时间，旧写入不得覆盖新状态。
///
/// 数据构造：同一工作空间的项目 A/B；A 先保存执行中快照，再保存待审快照；
///   最后尝试用更早时间覆盖。B 不保存状态。
/// 执行过程：重建 Store 后读取工作空间快照，再移除项目 A。
/// 预期结果：A 只保留新快照及 2000 毫秒时间；B 无快照；移除 A 后缓存级联删除。
#[test]
fn last_known_agent_status_is_scoped_ordered_and_removed_with_area() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let db = temp.path().join("app.sqlite");
    let root_a = temp.path().join("a");
    let root_b = temp.path().join("b");
    std::fs::create_dir(&root_a).expect("应创建 A");
    std::fs::create_dir(&root_b).expect("应创建 B");
    let catalog = DesktopWorkspaceStore::new(db.clone());
    let workspace = catalog.create("空间").expect("应创建工作空间");
    let area_a = catalog
        .add_project(&workspace.id, &root_a)
        .expect("应添加 A");
    let area_b = catalog
        .add_project(&workspace.id, &root_b)
        .expect("应添加 B");
    let store = DesktopAreaStatusStore::new(db.clone());
    let running = DesktopAgentStatus {
        node_id: 7,
        name: "Codex".into(),
        task: Some("实现登录".into()),
        state: "running_tool".into(),
        run_id: Some("run-1".into()),
        turn_id: Some("turn-1".into()),
    };
    let pending = DesktopAgentStatus {
        state: "waiting_approval".into(),
        ..running.clone()
    };

    assert!(
        store
            .save(&workspace.id, &area_a.id, 1000, &[running.clone()])
            .expect("应保存首次状态"),
        "首次状态应写入"
    );
    assert!(
        store
            .save(&workspace.id, &area_a.id, 2000, &[pending.clone()])
            .expect("应保存最新状态"),
        "新状态应替换旧状态"
    );
    assert!(
        !store
            .save(&workspace.id, &area_a.id, 1500, &[running])
            .expect("旧写入应被拒绝"),
        "较早状态不得覆盖待审"
    );

    let reopened = DesktopAreaStatusStore::new(db.clone());
    let statuses = reopened.list(&workspace.id).expect("应读取状态");
    assert_eq!(statuses.len(), 1, "只有 A 保存过状态，B 不得被伪造");
    assert_eq!(statuses[0].area_id, area_a.id, "状态必须归属 A");
    assert_eq!(
        statuses[0].updated_at_unix_ms, 2000,
        "应保留最近一次更新时间"
    );
    assert_eq!(
        statuses[0].agents,
        vec![pending],
        "待审状态不能被旧执行中状态覆盖"
    );
    assert!(
        !statuses.iter().any(|status| status.area_id == area_b.id),
        "B 不得出现 A 的状态"
    );
    assert!(
        catalog
            .remove_project(&workspace.id, &area_a.id)
            .expect("应移除 A"),
        "A 应成功移除"
    );
    assert!(
        reopened
            .list(&workspace.id)
            .expect("移除后应读取")
            .is_empty(),
        "移除 A 后旧状态必须删除"
    );
}
