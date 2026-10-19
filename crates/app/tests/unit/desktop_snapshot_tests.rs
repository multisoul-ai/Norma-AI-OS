use super::*;
use gpui::{AppContext as _, TestAppContext};
use storage::{DesktopAgentStatus, DesktopAreaStatus, DesktopWorkspaceStore};

/// 项目内的 Norma 查询工作空间进度时，不得重新读取正在更新的同一 GPUI 项目实体。
///
/// 数据构造：桌面宿主绑定一个已加载项目，项目画布有一张名为 Codex 的 Agent 卡片。
/// 执行过程：从该项目会话的 update 回调调用真实 workspace_snapshot 工具。
/// 预期结果：工具返回实时 Codex 状态；不触发 GPUI 的双重读取 panic，也不误报缓存状态。
#[test]
fn workspace_snapshot_from_project_session_uses_source_status_without_reentrant_read() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project-a");
    std::fs::create_dir(&root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace = catalog.create("工作空间").expect("应创建工作空间");
    let area = catalog
        .add_project(&workspace.id, &root)
        .expect("应添加项目");
    let prepared = crate::app::background_project::PreparedProject::load_for_desktop(
        paths.clone(),
        area.clone(),
    )
    .expect("应准备项目会话");
    let mut test = TestAppContext::single();
    let (host, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    let session = cx.new(|cx| {
        let mut app = MultisoulApp::from_background_project(prepared, cx);
        app.desktop_host = Some(host.downgrade());
        app
    });
    host.update(cx, |app, _| {
        app.screen = AppScreen::DesktopWorkspace;
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.workspaces = vec![workspace.clone()];
        app.desktop_workspace.areas = vec![area.clone()];
        app.desktop_workspace.expanded_area_id = Some(area.id.clone());
        app.desktop_workspace
            .sessions
            .insert((workspace.id.clone(), area.id.clone()), session.clone());
    });
    let snapshot = session.update(cx, |app, cx| {
        app.runtime.document.add_node(
            canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
                profile: canvas::AgentProfile::Codex,
                alias: Some("Codex".into()),
                definition_id: None,
                mobile_control: false,
            }),
            canvas::CanvasRect::new(0.0, 0.0, 640.0, 400.0),
        );
        app.norma_workspace_snapshot(WorkspaceSnapshotScope::Workspace, None, cx)
            .expect("项目内的工作空间查询应成功")
            .snapshot
            .expect("应返回结构化状态")
    });
    assert_eq!(
        snapshot["projects"][0]["live"], true,
        "当前项目应报告实时状态"
    );
    assert_eq!(
        snapshot["projects"][0]["agents"][0]["name"], "Codex",
        "实时状态应包含当前项目的 Agent 卡片"
    );
    assert_eq!(
        snapshot["projects"][0]["agents"][0]["state"], "idle",
        "没有运行任务的卡片不能被误报为执行中"
    );
}

/// SNAPSHOT-1：模型只读工具在工作空间总览返回真实缓存状态，不启动项目。
///
/// 数据构造：工作空间 dw 包含一个未加载项目；缓存中 Codex 的任务等待审批，时间戳为 2000。
/// 执行过程：在总览调用生产 workspace_snapshot，读取结构化回执。
/// 预期结果：返回稳定工作空间／项目身份、待审状态与缓存时间；项目会话保持为空。
#[test]
fn workspace_snapshot_reports_cached_approval_without_loading_project() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project-a");
    std::fs::create_dir(&root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace = catalog.create("产品研发").expect("应创建工作空间");
    let area = catalog
        .add_project(&workspace.id, &root)
        .expect("应添加项目");
    let mut test = TestAppContext::single();
    let (app, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    let result = app.update(cx, |app, cx| {
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
        app.norma_workspace_snapshot(WorkspaceSnapshotScope::CurrentProject, None, cx)
            .expect("总览无聚焦项目时应汇总当前工作空间")
    });
    let snapshot = result.snapshot.expect("工具应返回结构化快照");
    assert_eq!(
        snapshot["workspace_id"], workspace.id,
        "快照必须标明当前工作空间稳定 ID"
    );
    assert_eq!(
        snapshot["projects"][0]["area_id"], area.id,
        "快照必须标明项目区域稳定 ID"
    );
    assert_eq!(
        snapshot["projects"][0]["agents"][0]["state"], "waiting_approval",
        "缓存的上次待审状态应可供 Norma 查询"
    );
    assert_eq!(
        snapshot["projects"][0]["agents"][0]["needs_approval"], true,
        "上次待审状态应供 Norma 如实汇总"
    );
    assert_eq!(
        snapshot["projects"][0]["agents"][0]["can_locate_approval"], false,
        "未加载项目的审批请求不能宣称当前可处理"
    );
    assert_eq!(
        snapshot["projects"][0]["updated_at_unix_ms"], 2_000,
        "未加载项目应带最后更新时间"
    );
    assert_eq!(
        snapshot["projects"][0]["live"], false,
        "缓存状态不得被误报为实时状态"
    );
    app.read_with(cx, |app, _| {
        assert!(
            app.desktop_workspace.sessions.is_empty(),
            "只读查询不得启动项目会话"
        );
    });
}
