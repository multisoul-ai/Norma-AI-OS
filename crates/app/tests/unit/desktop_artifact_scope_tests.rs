use super::*;
use crate::app::AppScreen;
use gpui::{AppContext as _, TestAppContext};
use storage::{
    ApprovalAuditStore, BackgroundApprovalAudit, DesktopAgentStatus, DesktopAreaStatus,
    DesktopContentTabStore, DesktopWorkspaceStore, MobileTaskDeliveryStore, MobileTurnKey,
    MobileTurnOutcome, MobileTurnState, MobileTurnStore, TaskDeliveryContent,
};
use workspace::review::{ContentKind, FileChange, FileVersion, ProjectReview};

/// 场景：未加载项目的方案仍应显示来源 Agent，而不退化为“原 Agent”。
/// 数据构造：项目没有运行会话；节点 7 的存储来源类型为 Codex，最近状态缓存标记其别名 Alan。
/// 执行过程：先读取概览应显示的名称，再清除缓存模拟仅剩任务产物记录。
/// 预期结果：有缓存时显示 Alan；没有缓存时显示可信的 Codex 类型，不虚构别名。
#[test]
fn artifact_agent_name_uses_cached_alias_then_task_profile_when_project_is_unloaded() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace = catalog.create("空间").expect("应创建工作空间");
    let root = temp.path().join("project-b");
    std::fs::create_dir(&root).expect("应创建项目目录");
    let area = catalog
        .add_project(&workspace.id, &root)
        .expect("应添加项目");
    let artifact = DesktopArtifact {
        area_id: area.id.clone(),
        node_id: 7,
        agent_profile: "Codex".into(),
        turn_id: "turn-b".into(),
        path: root.join("plan.md"),
        source_path: "plan.md".into(),
        version: "version-b".into(),
        saved_at_unix: 1,
    };
    let mut test = TestAppContext::single();
    let (app, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    app.update(cx, |app, cx| {
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.areas = vec![area.clone()];
        app.desktop_workspace.status_cache.insert(
            area.id.clone(),
            DesktopAreaStatus {
                area_id: area.id.clone(),
                updated_at_unix_ms: 2_000,
                agents: vec![DesktopAgentStatus {
                    node_id: 7,
                    name: "Alan".into(),
                    task: Some("审核方案".into()),
                    state: "done".into(),
                    run_id: None,
                    turn_id: None,
                }],
            },
        );
        assert!(
            app.desktop_workspace.sessions.is_empty(),
            "测试项目必须保持未加载"
        );
        assert_eq!(
            app.desktop_artifact_agent_name(&artifact, cx),
            "Alan",
            "未加载项目的产物应显示缓存中的 Agent 别名"
        );
        app.desktop_workspace.status_cache.clear();
        assert_eq!(
            app.desktop_artifact_agent_name(&artifact, cx),
            "Codex",
            "没有最近状态时应退回任务记录中可信的 Agent 类型"
        );
    });
}

/// 工作空间切换期间完成的文件校验不能把旧空间的产物写进新空间标签。
///
/// 数据构造：A、B 两个独立工作空间，A 项目中有一个真实文件和任务产物引用。
/// 执行过程：从 A 请求打开产物，后台校验完成前立即将当前空间切到 B。
/// 预期结果：B 的内容标签和持久记录都保持为空，旧请求不能串写到 B。
#[test]
fn artifact_open_result_does_not_cross_workspace_after_switch() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace_a = catalog.create("A").expect("应创建 A");
    let workspace_b = catalog.create("B").expect("应创建 B");
    let root_a = temp.path().join("project-a");
    let root_b = temp.path().join("project-b");
    std::fs::create_dir(&root_a).expect("应创建 A 项目目录");
    std::fs::create_dir(&root_b).expect("应创建 B 项目目录");
    let area_a = catalog
        .add_project(&workspace_a.id, &root_a)
        .expect("应添加 A 项目");
    let area_b = catalog
        .add_project(&workspace_b.id, &root_b)
        .expect("应添加 B 项目");
    let path = root_a.join("plan.md");
    std::fs::write(&path, "# A plan").expect("应写入 A 的方案");
    let artifact = DesktopArtifact {
        area_id: area_a.id.clone(),
        node_id: 7,
        agent_profile: "Codex".into(),
        turn_id: "turn-a".into(),
        path,
        source_path: "plan.md".into(),
        version: "version-a".into(),
        saved_at_unix: 1,
    };
    let mut test = TestAppContext::single();
    let (app, cx) = test
        .add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store));
    app.update(cx, |app, cx| {
        app.desktop_workspace.active_id = Some(workspace_a.id.clone());
        app.desktop_workspace.areas = vec![area_a];
        app.open_desktop_artifact(artifact, cx);
        app.desktop_workspace.active_id = Some(workspace_b.id.clone());
        app.desktop_workspace.areas = vec![area_b];
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(
            app.desktop_workspace.content_tabs.is_empty(),
            "B 不应显示 A 的文件标签"
        );
    });
    let tabs = DesktopContentTabStore::new(paths.app_db_path().to_owned())
        .list(&workspace_b.id)
        .expect("应读取 B 的标签");
    assert!(tabs.is_empty(), "B 不应持久保存 A 的产物标签");
}

/// Norma 打开成果时只接受当前工作空间内、索引中同一回合的冻结版本。
///
/// 数据构造：项目 B 的 Agent 7 产生 plan.md，文件与冻结成果正文故意不同，当前保持总览。
/// 执行过程：先以错误版本请求，再以完整引用请求，等待文件校验与标签存储。
/// 预期结果：错误版本无副作用；正确引用在右侧打开标有 B 来源的文件，中央不切项目。
#[test]
fn norma_artifact_tool_opens_exact_version_in_right_drawer_without_switching_project() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace = catalog.create("空间").expect("应创建工作空间");
    let root = temp.path().join("project-b");
    std::fs::create_dir(&root).expect("应创建 B 项目目录");
    let area = catalog.add_project(&workspace.id, &root).expect("应添加 B");
    let path = area.root.join("plan.md");
    std::fs::write(&path, "# B plan").expect("应写入方案");
    let key = MobileTurnKey {
        node_id: 7,
        profile: "Codex".into(),
        run_id: "run-b".into(),
        session_id: "session-b".into(),
        turn_id: "turn-b".into(),
    };
    let source = format!("desktop:{}:{}", workspace.id, area.id);
    let writer = BackgroundApprovalAudit::new(
        ApprovalAuditStore::new(temp.path().join("audit.sqlite")).with_turn_store(
            MobileTurnStore::for_workspace(paths.app_db_path().to_owned(), source.clone()),
        ),
    )
    .expect("应创建任务结果 writer");
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
    let saved = MobileTaskDeliveryStore::new(paths.app_db_path().to_owned())
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
                            bytes: b"# Frozen B plan".to_vec(),
                        }),
                        diff: None,
                        staged_diff: None,
                    }],
                },
            },
        )
        .expect("应冻结任务方案版本");
    let artifact = DesktopArtifact {
        area_id: area.id.clone(),
        node_id: 7,
        agent_profile: "Codex".into(),
        turn_id: "turn-b".into(),
        path: path.clone(),
        source_path: "plan.md".into(),
        version: saved.fingerprint.clone(),
        saved_at_unix: 1,
    };
    let mut test = TestAppContext::single();
    let (app, cx) = test
        .add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store));
    app.update(cx, |app, cx| {
        // 冻结 Markdown 现在使用富文本 WebView；本单测只验版本与来源，原生窗口由 desktop-e2e 验收。
        app.browsers.disable_native_views();
        app.screen = AppScreen::DesktopWorkspace;
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.workspaces = vec![workspace.clone()];
        app.desktop_workspace.areas = vec![area.clone()];
        app.desktop_workspace.content_artifacts = vec![artifact];
        let stale = app.norma_open_workspace_artifact(
            &workspace.id,
            &area.id,
            7,
            "turn-b",
            "plan.md",
            "wrong-version",
            cx,
        );
        assert!(stale.is_err(), "版本不一致时应拒绝打开");
        assert!(
            app.desktop_workspace.content_tabs.is_empty(),
            "拒绝后不能创建标签"
        );
        let accepted = app
            .norma_open_workspace_artifact(
                &workspace.id,
                &area.id,
                7,
                "turn-b",
                "plan.md",
                &saved.fingerprint,
                cx,
            )
            .expect("完整引用应触发打开");
        assert_eq!(accepted.status, "accepted", "异步打开应先返回接收回执");
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.content_tabs.len(),
            1,
            "应有一个成果标签"
        );
        let tab = &app.desktop_workspace.content_tabs[0];
        assert_eq!(
            tab.resource,
            path.to_string_lossy(),
            "标签应指向真实方案文件"
        );
        assert_eq!(
            tab.source_area_id.as_deref(),
            Some(area.id.as_str()),
            "标签应标记 B 来源"
        );
        assert!(tab.selected, "新打开的成果标签应选中");
        assert!(app.desktop_workspace.drawers.right_open, "应展开右侧抽屉");
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "中央画布不得切到 B"
        );
        assert_eq!(
            app.desktop_workspace.content_previews.get(&tab.id),
            Some(&Ok("# Frozen B plan".into())),
            "右侧必须真正加载冻结成果正文，不能只创建空标签"
        );
    });
}

/// 桌面项目里旧网页与文件工具应打开右侧标签，中央不产生不可见内容卡片。
///
/// 数据构造：工作空间加载一个真实项目及 plan.md，项目会话绑定桌面宿主。
/// 执行过程：通过 Norma 生产 Adapter 分别执行 create_browser 与 create_preview。
/// 预期结果：宿主右侧有网页和文件标签；项目画布不新增 Browser／Preview 节点。
#[test]
fn desktop_norma_browser_and_preview_tools_open_right_tabs_only() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace = catalog.create("空间").expect("应创建工作空间");
    let root = temp.path().join("project-a");
    std::fs::create_dir(&root).expect("应创建项目目录");
    let area = catalog
        .add_project(&workspace.id, &root)
        .expect("应添加项目");
    std::fs::write(area.root.join("plan.md"), "# Current plan").expect("应创建可打开文件");
    let prepared = crate::app::background_project::PreparedProject::load_for_desktop(
        paths.clone(),
        area.clone(),
    )
    .expect("应准备项目实例");
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
    let browser = session.update_in(cx, |app, window, cx| {
        crate::app::execute_norma_adapter_command_with_source(
            app,
            Some(window),
            cx,
            agent::AdapterCommand::CreateBrowser {
                url: Some("https://example.com".into()),
            },
            agent::AgentInputSource::Voice,
        )
        .expect("桌面网页工具应接收")
    });
    let preview = session.update_in(cx, |app, window, cx| {
        crate::app::execute_norma_adapter_command_with_source(
            app,
            Some(window),
            cx,
            agent::AdapterCommand::CreatePreview {
                path: "plan.md".into(),
                explicit_type: None,
            },
            agent::AgentInputSource::Voice,
        )
        .expect("桌面预览工具应接收")
    });
    assert_eq!(browser.status, "accepted", "网页打开为异步请求");
    assert_eq!(preview.status, "accepted", "文件打开为异步请求");
    // Headless TestAppContext 没有可供 Wry 创建 WebView 的原生窗口。
    // 工具调用已在桌面模式完成；异步存储回调期间只渲染 Launchpad，
    // 由真实 App 的 desktop-e2e 覆盖右栏网页画面的验收。
    host.update(cx, |app, _| app.screen = AppScreen::Launchpad);
    cx.run_until_parked();
    host.read_with(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.content_tabs.len(),
            2,
            "右侧应有两个内容标签"
        );
        assert!(
            app.desktop_workspace.content_tabs.iter().any(|tab| {
                tab.kind == storage::DesktopContentKind::Browser
                    && tab.resource == "https://example.com/"
            }),
            "网页应成为右侧 Browser 标签"
        );
        assert!(
            app.desktop_workspace.content_tabs.iter().any(|tab| {
                tab.kind == storage::DesktopContentKind::File && tab.resource.ends_with("plan.md")
            }),
            "方案应成为右侧 File 标签"
        );
    });
    session.read_with(cx, |app, _| {
        assert!(
            !app.runtime.document.nodes().iter().any(|node| matches!(
                node.kind,
                canvas::CanvasNodeKind::Browser(_) | canvas::CanvasNodeKind::Preview(_)
            )),
            "中央画布不得留下隐藏 Browser／Preview 卡片"
        );
    });
}
