use super::*;
use crate::app::AppScreen;
use agent::WorkspaceSnapshotScope;
use gpui::TestAppContext;
use storage::{
    ApprovalAuditStore, BackgroundApprovalAudit, DesktopAgentStatus, DesktopAreaStatus,
    DesktopWorkspaceStore, MobileTaskDeliveryStore, MobileTurnKey, MobileTurnOutcome,
    MobileTurnState, MobileTurnStore, TaskDeliveryContent,
};
use workspace::review::{ContentKind, FileChange, FileVersion, ProjectReview};

/// 尚未加载的项目不启动 Agent，也要在只读工具快照中标明最后状态与更新时间。
///
/// 数据构造：隔离工作空间含 A 项目；A 没有项目会话，但有一条已持久化的待审快照。
/// 执行过程：在桌面工作空间查询整个工作空间快照。
/// 预期结果：快照含 A、待审核及上次更新时间；不得称为实时状态或启动项目会话。
#[test]
fn unloaded_project_reports_last_known_status_without_starting_session() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project-a");
    std::fs::create_dir(&root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace = catalog.create("空间").expect("应创建空间");
    let area = catalog.add_project(&workspace.id, &root).expect("应添加 A");
    let mut test = TestAppContext::single();
    let (app, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    let answer = app.update(cx, |app, cx| {
        app.screen = AppScreen::DesktopWorkspace;
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.workspaces = vec![workspace.clone()];
        app.desktop_workspace.areas = vec![area.clone()];
        app.desktop_workspace.status_cache.insert(
            area.id.clone(),
            DesktopAreaStatus {
                area_id: area.id.clone(),
                updated_at_unix_ms: 2_000,
                agents: vec![DesktopAgentStatus {
                    node_id: 7,
                    name: "Codex".into(),
                    task: Some("审核登录方案".into()),
                    state: "waiting_approval".into(),
                    run_id: Some("run-1".into()),
                    turn_id: Some("turn-1".into()),
                }],
            },
        );
        app.norma_workspace_snapshot(WorkspaceSnapshotScope::Workspace, None, cx)
            .expect("进度查询应产生只读快照")
            .snapshot
            .expect("工具应返回结构化快照")
    });
    let project = &answer["projects"][0];
    assert_eq!(project["area_id"], area.id, "快照应指向 A 项目");
    assert_eq!(project["agents"][0]["name"], "Codex", "应保留 Agent 名称");
    assert_eq!(
        project["agents"][0]["needs_approval"], true,
        "上次待审状态仍应如实汇总"
    );
    assert_eq!(
        project["agents"][0]["can_locate_approval"], false,
        "未加载项目不能宣称审批入口当前可用"
    );
    assert_eq!(project["updated_at_unix_ms"], 2_000, "应标明缓存更新时间");
    assert_eq!(project["live"], false, "旧快照不得被误称为实时状态");
    app.read_with(cx, |app, _| {
        assert!(
            app.desktop_workspace.sessions.is_empty(),
            "查询不得自动启动未加载项目"
        );
    });
}

/// 工作空间重新打开时应从存储恢复未加载项目状态，不能只依赖内存注入。
///
/// 数据构造：A 的任务状态已写入隔离数据库，然后移走 A 的项目目录以阻止会话加载。
/// 执行过程：经正式工作空间打开入口读取目录和状态，再查询全空间快照。
/// 预期结果：查询仍包含 A 的上次待审状态和更新时间；项目会话保持未加载。
#[test]
fn opening_workspace_restores_unloaded_project_status_from_storage() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project-a");
    std::fs::create_dir(&root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace = catalog.create("空间").expect("应创建空间");
    let area = catalog.add_project(&workspace.id, &root).expect("应添加 A");
    let cache = storage::DesktopAreaStatusStore::new(paths.app_db_path().to_owned());
    assert!(
        cache
            .save(
                &workspace.id,
                &area.id,
                2_000,
                &[DesktopAgentStatus {
                    node_id: 7,
                    name: "Codex".into(),
                    task: Some("审核登录方案".into()),
                    state: "waiting_approval".into(),
                    run_id: Some("run-1".into()),
                    turn_id: Some("turn-1".into()),
                }],
            )
            .expect("应写入上次状态"),
        "首次快照应保存成功"
    );
    std::fs::remove_dir(&root).expect("应移走项目目录");
    let mut test = TestAppContext::single();
    let (app, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    app.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = true;
        app.open_desktop_workspace(workspace.id.clone(), window, cx);
    });
    cx.run_until_parked();
    let answer = app.update(cx, |app, cx| {
        app.norma_workspace_snapshot(WorkspaceSnapshotScope::Workspace, None, cx)
            .expect("应返回全空间快照")
            .snapshot
            .expect("工具应返回结构化快照")
    });
    let project = &answer["projects"][0];
    assert_eq!(project["area_id"], area.id, "应恢复 A 项目");
    assert_eq!(
        project["agents"][0]["needs_approval"], true,
        "恢复的上次待审状态仍应如实显示"
    );
    assert_eq!(
        project["agents"][0]["can_locate_approval"], false,
        "恢复的旧状态不能被当成仍可处理的审批请求"
    );
    assert_eq!(project["updated_at_unix_ms"], 2_000, "应恢复缓存更新时间");
    assert_eq!(project["live"], false, "未加载项目不得宣称状态实时");
    app.read_with(cx, |app, _| {
        assert!(
            app.desktop_workspace
                .sessions
                .get(&(workspace.id.clone(), area.id.clone()))
                .is_none(),
            "项目目录不可用时不得启动会话"
        );
    });
}

/// 所有项目未加载时，周期状态刷新仍须读取已存的待看产物索引。
///
/// 数据构造：隔离工作空间有一个有效项目目录、已完成回合及真实 plan.md，
///   但没有任何项目会话。
/// 执行过程：启动桌面刷新时钟并推进一轮 500ms 定时器。
/// 预期结果：本次产物查询修订号被消费，概览取到明确来源的 plan.md，
///   且项目会话仍为空。
#[test]
fn status_tick_refreshes_artifacts_without_loaded_sessions() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project-a");
    std::fs::create_dir(&root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace = catalog.create("空间").expect("应创建空间");
    let area = catalog
        .add_project(&workspace.id, &root)
        .expect("应添加项目");
    std::fs::write(root.join("plan.md"), "# 可查看方案").expect("应创建真实方案文件");
    let source = format!("desktop:{}:{}", workspace.id, area.id);
    let key = MobileTurnKey {
        node_id: 7,
        profile: "Codex".into(),
        run_id: "run-a".into(),
        session_id: "session-a".into(),
        turn_id: "turn-a".into(),
    };
    let db = paths.app_db_path().to_owned();
    let writer = BackgroundApprovalAudit::new(
        ApprovalAuditStore::new(temp.path().join("audit.sqlite"))
            .with_turn_store(MobileTurnStore::for_workspace(db.clone(), source.clone())),
    )
    .expect("应创建已完成回合 writer");
    writer
        .record_turn(MobileTurnOutcome {
            summary: None,
            key: key.clone(),
            state: MobileTurnState::Completed,
        })
        .expect("应写入任务终态")
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("应收到写入回执")
        .expect("任务终态应落盘");
    let saved = MobileTaskDeliveryStore::new(db)
        .record(
            &source,
            &key,
            &TaskDeliveryContent::Available {
                review: ProjectReview {
                    base_commit: None,
                    conflicted: false,
                    files: vec![FileChange {
                        path: "plan.md".into(),
                        status: "??".into(),
                        before: None,
                        index: None,
                        after: Some(FileVersion {
                            kind: ContentKind::Text,
                            executable: false,
                            bytes: b"# reviewable plan".to_vec(),
                        }),
                        diff: None,
                        staged_diff: None,
                    }],
                },
            },
        )
        .expect("应冻结方案产物");
    let area_id = area.id.clone();
    let mut test = TestAppContext::single();
    let (app, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    // 事件修订号由整个测试进程共享。只要求本窗口消费自己启动前的更新，
    // 其他并行用例随后产生的更新不属于这一轮 500ms 的断言范围。
    crate::mobile::project_events::changed();
    let expected_revision = crate::mobile::project_events::revision();
    app.update_in(cx, |app, window, cx| {
        // 渲染会从测试窗口同步活动状态；激活真实测试窗口后再推进刷新时钟。
        window.activate_window();
        app.screen = AppScreen::DesktopWorkspace;
        app.desktop_workspace.active_id = Some(workspace.id);
        app.desktop_workspace.areas = vec![area];
        app.start_desktop_status_refresh(cx);
    });
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(500));
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(
            app.desktop_workspace.artifact_revision >= expected_revision,
            "空会话空间应消费本次更新后的待看产物索引：期望至少 {expected_revision}，实际 {}",
            app.desktop_workspace.artifact_revision
        );
        assert!(
            app.desktop_workspace.artifact_revision <= crate::mobile::project_events::revision(),
            "已消费的产物修订号不能超前于全局事件号"
        );
        assert_eq!(
            app.desktop_workspace.content_artifacts.len(),
            1,
            "空会话空间仍应读取一个有任务来源的方案"
        );
        let artifact = &app.desktop_workspace.content_artifacts[0];
        assert_eq!(artifact.area_id, area_id, "产物应保留来源项目");
        assert_eq!(artifact.node_id, 7, "产物应保留来源 Agent 节点");
        assert_eq!(artifact.turn_id, "turn-a", "产物应保留来源回合");
        assert_eq!(artifact.source_path, "plan.md", "产物应有可打开路径");
        assert_eq!(artifact.version, saved.fingerprint, "产物应绑定冻结版本");
        assert!(
            app.desktop_workspace.sessions.is_empty(),
            "读取产物不得自动启动项目会话"
        );
    });
}

/// Ego 画面隐藏后仍须在工作空间概览与只读快照中显示真实任务阶段。
///
/// 数据构造：已加载的项目包含一个 Ego Lite 任务，状态依次进入就绪、绑定 Agent、执行中。
/// 执行过程：在宿主工作空间查询全空间快照。
/// 预期结果：快照包含 Ego Lite 的执行状态，中央隐藏画面不会令任务从概览消失。
#[test]
fn hidden_ego_task_remains_in_live_status_projection() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project-a");
    std::fs::create_dir(&root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace = catalog.create("空间").expect("应创建空间");
    let area = catalog
        .add_project(&workspace.id, &root)
        .expect("应添加项目");
    let mut test = TestAppContext::single();
    let (host, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    host.update_in(cx, |app, window, cx| {
        app.screen = AppScreen::DesktopWorkspace;
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.workspaces = vec![workspace.clone()];
        app.desktop_workspace.areas = vec![area.clone()];
        app.ensure_desktop_area_session(area.id.clone(), window, cx);
    });
    cx.run_until_parked();
    let project = host.read_with(cx, |app, _| {
        app.desktop_workspace.sessions[&(workspace.id.clone(), area.id.clone())].clone()
    });
    project.update(cx, |app, _| {
        let ego = app.runtime.spawn_ego_browser(
            Some("网页任务".into()),
            canvas::CanvasRect::new(0.0, 0.0, 400.0, 300.0),
        );
        app.runtime
            .apply_ego_task_command(
                ego,
                runtime::EgoTaskCommand::TaskSpaceReady {
                    task_space_id: "space-1".into(),
                    task_space_name: "网页任务".into(),
                },
            )
            .expect("Ego 应就绪");
        app.runtime
            .apply_ego_task_command(
                ego,
                runtime::EgoTaskCommand::PrepareAgent { agent_node_id: 900 },
            )
            .expect("Ego 应绑定 Agent");
        app.runtime
            .apply_ego_task_command(
                ego,
                runtime::EgoTaskCommand::AgentRunning { agent_node_id: 900 },
            )
            .expect("Ego 应进入执行中");
    });
    let answer = host.update(cx, |app, cx| {
        app.norma_workspace_snapshot(WorkspaceSnapshotScope::Workspace, None, cx)
            .expect("应返回进度快照")
            .snapshot
            .expect("工具应返回结构化快照")
    });
    let agent = &answer["projects"][0]["agents"][0];
    assert_eq!(agent["name"], "Ego Lite", "隐藏的 Ego 任务仍应可见");
    assert_eq!(agent["state"], "ego_working", "执行阶段必须与 Runtime 一致");
}
