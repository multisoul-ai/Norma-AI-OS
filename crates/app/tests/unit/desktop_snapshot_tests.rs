use super::*;
use gpui::TestAppContext;
use storage::{DesktopAgentStatus, DesktopAreaStatus, DesktopWorkspaceStore};

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
