use super::*;
use gpui::TestAppContext;
use std::time::Duration;
use storage::{DesktopContentTabStore, DesktopWorkspaceStore};
use workspace::review::{ContentKind, FileChange, FileVersion, ProjectReview};

/// 连续标签操作必须按用户发起顺序落盘并回填，不可让旧回调复活已关闭标签。
///
/// 数据构造：隔离工作空间里先打开 A、B 两个网页标签。
/// 执行过程：不等待选择 A 完成就关闭 A，再等待后台操作全部结束。
/// 预期结果：只有 B 留在界面与数据库中，且 B 为选中标签。
#[test]
fn content_tab_operations_preserve_request_order() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace = catalog.create("空间").expect("应创建工作空间");
    let mut test = TestAppContext::single();
    let (app, cx) = test
        .add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store));
    app.update(cx, |app, cx| {
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.open_desktop_content_tab(
            DesktopContentKind::Browser,
            "https://a.example.test".into(),
            "A".into(),
            None,
            cx,
        );
        app.open_desktop_content_tab(
            DesktopContentKind::Browser,
            "https://b.example.test".into(),
            "B".into(),
            None,
            cx,
        );
    });
    cx.run_until_parked();
    let a_id = app.read_with(cx, |app, _| {
        let tabs = &app.desktop_workspace.content_tabs;
        assert_eq!(tabs.len(), 2, "两次打开必须保留两个标签");
        assert!(
            tabs.iter().any(|tab| tab.title == "B" && tab.selected),
            "最后打开的 B 应选中"
        );
        tabs.iter()
            .find(|tab| tab.title == "A")
            .expect("A 标签应存在")
            .id
            .clone()
    });
    app.update(cx, |app, cx| {
        app.select_desktop_content_tab(&a_id, cx);
        app.close_desktop_content_tab(&a_id, cx);
    });
    cx.run_until_parked();
    let saved = DesktopContentTabStore::new(paths.app_db_path().to_owned())
        .list(&workspace.id)
        .expect("应读取最终标签");
    assert_eq!(saved.len(), 1, "持久层应只剩 B 标签");
    assert_eq!(saved[0].title, "B", "A 关闭后不能重新出现");
    assert!(saved[0].selected, "剩余 B 应自动选中");
    app.read_with(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.content_tabs, saved,
            "界面应与持久层一致"
        );
    });
}

/// 右栏浏览器收到页面导航后，绘制所用地址必须先更新，再异步保存到空间标签。
///
/// 数据构造：隔离空间内一个已选中的 Browser 标签，原地址为 A。
/// 执行过程：同一帧连续报告地址 B、C，先检查内存，再等待按序后台存储。
/// 预期结果：本帧绘制读取 C 而不会回到 A 或 B；重启读出的标签资源也是 C。
#[test]
fn browser_navigation_updates_drawer_resource_before_persisting() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let workspace = DesktopWorkspaceStore::new(paths.app_db_path().to_owned())
        .create("导航空间")
        .expect("应创建空间");
    let saved = DesktopContentTabStore::new(paths.app_db_path().to_owned())
        .open_or_select(storage::NewDesktopContentTab {
            workspace_id: workspace.id.clone(),
            source_area_id: None,
            source_project_name: "导航空间".into(),
            kind: DesktopContentKind::Browser,
            resource: "https://a.example.test/".into(),
            version: None,
            title: "A".into(),
        })
        .expect("应打开原始网页");
    let mut test = TestAppContext::single();
    let (app, cx) = test
        .add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store));
    app.update(cx, |app, cx| {
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.content_tabs = vec![saved.clone()];
        assert!(
            app.update_desktop_browser_tab_location(
                super::super::content_node_id(&saved.id),
                "https://b.example.test/".into(),
                cx,
            ),
            "Browser 地址事件应由右栏标签接收"
        );
        assert_eq!(
            app.desktop_workspace.content_tabs[0].resource, "https://b.example.test/",
            "同一帧的浏览器绘制必须读取新地址"
        );
        assert!(
            app.update_desktop_browser_tab_location(
                super::super::content_node_id(&saved.id),
                "https://c.example.test/".into(),
                cx,
            ),
            "连续重定向应继续更新当前标签"
        );
        assert_eq!(
            app.desktop_workspace.content_tabs[0].resource, "https://c.example.test/",
            "连续导航不应被尚未完成的第一次落盘回调覆盖"
        );
    });
    cx.run_until_parked();
    let reopened = DesktopContentTabStore::new(paths.app_db_path().to_owned())
        .list(&workspace.id)
        .expect("应读取持久标签");
    assert_eq!(reopened.len(), 1, "导航不应新建标签");
    assert_eq!(
        reopened[0].resource, "https://c.example.test/",
        "重启后应恢复最后一次导航的 URL"
    );
}

/// 后台标签迟到导航到当前网页时，右栏要继续显示已选中的当前标签。
///
/// 数据构造：同一工作空间中 B 后台、A 已选中，两个 Browser 标签均已载入内存。
/// 执行过程：B 的浏览器节点报告跳转到 A 的 URL，再等待存储合并回调。
/// 预期结果：界面与数据库都只保留选中的 A；后台 B 不可替换当前内容。
#[test]
fn background_browser_duplicate_location_keeps_visible_tab_selected() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let workspace = DesktopWorkspaceStore::new(paths.app_db_path().to_owned())
        .create("后台网页")
        .expect("应创建空间");
    let tabs = DesktopContentTabStore::new(paths.app_db_path().to_owned());
    let open = |resource: &str| {
        tabs.open_or_select(storage::NewDesktopContentTab {
            workspace_id: workspace.id.clone(),
            source_area_id: None,
            source_project_name: "后台网页".into(),
            kind: DesktopContentKind::Browser,
            resource: resource.into(),
            version: None,
            title: resource.into(),
        })
        .expect("应打开标签")
    };
    let background = open("https://b.example.test/");
    let selected = open("https://a.example.test/");
    let mut test = TestAppContext::single();
    let (app, cx) = test
        .add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store));
    app.update(cx, |app, cx| {
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.content_tabs = vec![background.clone(), selected.clone()];
        assert!(
            app.update_desktop_browser_tab_location(
                super::super::content_node_id(&background.id),
                selected.resource.clone(),
                cx,
            ),
            "后台网页地址变化应进入去重队列"
        );
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.content_tabs.len(),
            1,
            "界面应合并重复标签"
        );
        assert_eq!(
            app.desktop_workspace.content_tabs[0].id, selected.id,
            "当前可见标签身份不得被后台导航替换"
        );
        assert!(
            app.desktop_workspace.content_tabs[0].selected,
            "右栏必须保留一个选中标签"
        );
    });
    let persisted = tabs.list(&workspace.id).expect("应读取持久标签");
    assert_eq!(persisted.len(), 1, "数据库应只保留当前网页");
    assert_eq!(persisted[0].id, selected.id, "持久层也应保留选中的 A");
}

/// 网页导航合并和用户点击旧标签交错时，不能留下无法操作的幽灵标签。
///
/// 数据构造：A 是未选中标签，B 是当前标签；B 即将跳转到 A 的地址。
/// 执行过程：同一帧先提交 B 的导航，再点击仍在界面上的 A，等待顺序任务结束。
/// 预期结果：已删除的 A 不留在界面；点击等价地址后仍有一个选中的 B 标签。
#[test]
fn browser_merge_then_click_duplicate_selects_survivor() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let workspace = DesktopWorkspaceStore::new(paths.app_db_path().to_owned())
        .create("竞态空间")
        .expect("应创建空间");
    let tabs = DesktopContentTabStore::new(paths.app_db_path().to_owned());
    let open = |resource: &str| {
        tabs.open_or_select(storage::NewDesktopContentTab {
            workspace_id: workspace.id.clone(),
            source_area_id: None,
            source_project_name: "竞态空间".into(),
            kind: DesktopContentKind::Browser,
            resource: resource.into(),
            version: None,
            title: resource.into(),
        })
        .expect("应打开标签")
    };
    let old_a = open("https://a.example.test/");
    let current_b = open("https://b.example.test/");
    let mut test = TestAppContext::single();
    let (app, cx) = test
        .add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store));
    app.update(cx, |app, cx| {
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.content_tabs = vec![old_a.clone(), current_b.clone()];
        assert!(
            app.update_desktop_browser_tab_location(
                super::super::content_node_id(&current_b.id),
                old_a.resource.clone(),
                cx,
            ),
            "B 的导航应入队"
        );
        app.select_desktop_content_tab(&old_a.id, cx);
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        let visible = &app.desktop_workspace.content_tabs;
        assert_eq!(visible.len(), 1, "界面不能留下已删除的 A 幽灵标签");
        assert_eq!(visible[0].id, current_b.id, "等价地址应落到存活的 B 标签");
        assert!(visible[0].selected, "点击后右栏必须仍有选中内容");
    });
    let saved = tabs.list(&workspace.id).expect("应读取数据库最终标签");
    assert_eq!(saved.len(), 1, "数据库应合并成一个地址标签");
    assert_eq!(saved[0].id, current_b.id, "存活标签身份应保持 B");
    assert!(saved[0].selected, "持久层应保持选中状态");
}

/// 任务成果标签重开时显示冻结版本，不能把同一路径的后续内容冒充旧方案。
///
/// 数据构造：真实完成回合冻结 plan.md 的 v1，随后磁盘同路径改为 v2；标签保存来源指纹。
/// 执行过程：重新载入标签并触发右栏内容加载。
/// 预期结果：标签正文为 v1，磁盘仍为 v2，证明读取的是任务交付快照。
#[test]
fn artifact_tab_reopens_frozen_file_version() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let app_store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace = catalog.create("空间").expect("应创建工作空间");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("应创建项目目录");
    let area = catalog
        .add_project(&workspace.id, &root)
        .expect("应添加项目");
    let path = root.join("plan.md");
    std::fs::write(&path, "version one").expect("应写入 v1");
    let scope = format!("desktop:{}:{}", workspace.id, area.id);
    let key = storage::MobileTurnKey {
        node_id: 7,
        profile: "Codex".into(),
        run_id: "run-a".into(),
        session_id: "session-a".into(),
        turn_id: "turn-a".into(),
    };
    let writer = storage::BackgroundApprovalAudit::new(
        storage::ApprovalAuditStore::new(temp.path().join("audit.sqlite")).with_turn_store(
            storage::MobileTurnStore::for_workspace(paths.app_db_path().to_owned(), scope.clone()),
        ),
    )
    .expect("应创建终态 writer");
    writer
        .record_turn(storage::MobileTurnOutcome {
            summary: None,
            key: key.clone(),
            state: storage::MobileTurnState::Completed,
        })
        .expect("应入队")
        .recv_timeout(Duration::from_secs(5))
        .expect("应收到回执")
        .expect("应保存终态");
    let delivery = storage::MobileTaskDeliveryStore::new(paths.app_db_path().to_owned())
        .record(
            &scope,
            &key,
            &storage::TaskDeliveryContent::Available {
                review: ProjectReview {
                    base_commit: None,
                    conflicted: false,
                    files: vec![FileChange {
                        path: "plan.md".into(),
                        status: "??".into(),
                        before: None,
                        after: Some(FileVersion {
                            kind: ContentKind::Text,
                            executable: false,
                            bytes: b"version one".to_vec(),
                        }),
                        index: None,
                        diff: None,
                        staged_diff: None,
                    }],
                },
            },
        )
        .expect("应冻结任务成果");
    std::fs::write(&path, "version two").expect("应更新同一路径");
    let tab_store = DesktopContentTabStore::new(paths.app_db_path().to_owned());
    let tab = tab_store
        .open_or_select(storage::NewDesktopContentTab {
            workspace_id: workspace.id.clone(),
            source_area_id: Some(area.id.clone()),
            source_project_name: area.display_name.clone(),
            kind: DesktopContentKind::File,
            resource: path.to_string_lossy().into_owned(),
            version: Some(
                serde_json::to_string(&crate::workspace::desktop::DesktopArtifactVersion {
                    node_id: 7,
                    fingerprint: delivery.fingerprint,
                    source_path: "plan.md".into(),
                })
                .expect("应编码成果版本"),
            ),
            title: "plan.md".into(),
        })
        .expect("应保存版本标签");
    let mut test = TestAppContext::single();
    let (app, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, app_store));
    app.update(cx, |app, cx| {
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.areas = vec![area];
        app.desktop_workspace.content_tabs = vec![tab.clone()];
        app.load_selected_desktop_content(cx);
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.content_previews.get(&tab.id),
            Some(&Ok("version one".into())),
            "标签必须显示来源回合冻结的 v1"
        );
    });
    assert_eq!(
        std::fs::read_to_string(path).expect("应读取当前文件"),
        "version two",
        "预览不应改写磁盘当前文件"
    );
}

/// 旧工作空间项目的迟到打开请求不能误落到当前工作空间的右栏。
///
/// 数据构造：A、B 两个空间，A 项目会话仍在后台，宿主当前切到 B。
/// 执行过程：A 会话迟到请求打开网页。
/// 预期结果：请求被安全消费，B 的界面和数据库均不出现 A 的网页标签。
#[test]
fn background_project_content_does_not_open_in_new_workspace() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace_a = catalog.create("A").expect("应创建 A");
    let workspace_b = catalog.create("B").expect("应创建 B");
    let root = temp.path().join("project-a");
    std::fs::create_dir(&root).expect("应创建项目目录");
    let area_a = catalog
        .add_project(&workspace_a.id, &root)
        .expect("应添加 A 项目");
    let mut test = TestAppContext::single();
    let (host, cx) = test
        .add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store));
    host.update_in(cx, |app, window, cx| {
        app.desktop_workspace.active_id = Some(workspace_a.id.clone());
        app.desktop_workspace.areas = vec![area_a.clone()];
        app.ensure_desktop_area_session(area_a.id.clone(), window, cx);
    });
    cx.run_until_parked();
    let project = host.read_with(cx, |app, _| {
        app.desktop_workspace.sessions[&(workspace_a.id.clone(), area_a.id.clone())].clone()
    });
    host.update(cx, |app, _| {
        app.desktop_workspace.active_id = Some(workspace_b.id.clone());
        app.desktop_workspace.areas.clear();
    });
    let handled = project.update(cx, |app, cx| {
        app.route_project_content_to_desktop(
            DesktopContentKind::Browser,
            "https://a.example.test".into(),
            "A 网页".into(),
            cx,
        )
    });
    cx.run_until_parked();
    assert!(
        handled,
        "旧桌面会话的请求应被安全消费，不能回退生成画布卡片"
    );
    host.read_with(cx, |app, _| {
        assert!(
            app.desktop_workspace.content_tabs.is_empty(),
            "B 不应显示 A 的网页"
        );
    });
    let tabs = DesktopContentTabStore::new(paths.app_db_path().to_owned())
        .list(&workspace_b.id)
        .expect("应读取 B 标签");
    assert!(tabs.is_empty(), "B 不应持久保存 A 的网页");
}
