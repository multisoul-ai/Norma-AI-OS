use super::*;
use crate::app::background_project::PreparedProject;
use crate::app::AppScreen;
use gpui::{AppContext as _, TestAppContext};
use std::sync::{Arc, Mutex};

/// 稳定 ID 派活工具必须写入指定项目的 Agent，并留下逐目标回执。
///
/// 数据构造：一个桌面空间含 ProjectA 和 ProjectB，各有一张 Codex 卡片；
/// 两个隔离 Recording PTY 分别收集写入，当前聚焦 ProjectA。
/// 执行过程：拒绝错误工作空间后向 B 派活，再分别向 A/B 派活；
/// 停止 A 的 PTY 后分别重试，最后用无效节点 ID 验证过期目标。
/// 预期结果：任务只送达指定 PTY 一次；A 失败不阻止 B 成功；
/// 逐目标回执准确，过期目标不得产生 PTY 写入。
#[test]
fn workspace_task_tool_targets_exact_agent_and_isolates_failures() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root_a = temp.path().join("ProjectA");
    let root_b = temp.path().join("ProjectB");
    std::fs::create_dir(&root_a).expect("应创建 A 项目目录");
    std::fs::create_dir(&root_b).expect("应创建 B 项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Dispatch").expect("应创建桌面空间");
    let area_a = catalog
        .add_project(&workspace.id, &root_a)
        .expect("应添加 A 项目");
    let area_b = catalog
        .add_project(&workspace.id, &root_b)
        .expect("应添加 B 项目");
    let prepared_a = PreparedProject::load_for_desktop(paths.clone(), area_a.clone())
        .expect("应准备 A 项目实例");
    let prepared_b = PreparedProject::load_for_desktop(paths.clone(), area_b.clone())
        .expect("应准备 B 项目实例");
    let mut test = TestAppContext::single();
    let (host, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    let writes_a = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let writes_b = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let make_session = |prepared: PreparedProject,
                        writes: Arc<Mutex<Vec<Vec<u8>>>>,
                        cx: &mut gpui::VisualTestContext| {
        cx.new(|cx| {
            let mut app = MultisoulApp::from_background_project(prepared, cx);
            app.runtime.set_pty_factory(Box::new(move |_, _, _, tx| {
                Ok(Box::new(terminal::TestPtyBackend::recording(
                    tx,
                    writes.clone(),
                )))
            }));
            app
        })
    };
    let session_a = make_session(prepared_a, writes_a.clone(), cx);
    let session_b = make_session(prepared_b, writes_b.clone(), cx);
    host.update(cx, |app, _| {
        app.screen = AppScreen::DesktopWorkspace;
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.areas = vec![area_a.clone(), area_b.clone()];
        app.desktop_workspace.expanded_area_id = Some(area_a.id.clone());
        app.desktop_workspace
            .sessions
            .insert((workspace.id.clone(), area_a.id.clone()), session_a.clone());
        app.desktop_workspace
            .sessions
            .insert((workspace.id.clone(), area_b.id.clone()), session_b.clone());
    });
    session_a.update_in(cx, |app, window, cx| {
        app.spawn_agent_card(canvas::AgentProfile::Codex, window, cx);
    });
    session_b.update_in(cx, |app, window, cx| {
        app.spawn_agent_card(canvas::AgentProfile::Codex, window, cx);
    });
    cx.run_until_parked();
    for (label, session) in [("A", &session_a), ("B", &session_b)] {
        session.read_with(cx, |app, _| {
            assert_eq!(
                app.runtime
                    .document
                    .nodes()
                    .iter()
                    .filter(|node| matches!(node.kind, canvas::CanvasNodeKind::Agent(_)))
                    .count(),
                1,
                "{label} 必须实际创建一张 Agent 卡片"
            );
        });
    }
    writes_a.lock().expect("应读取 A 写入").clear();
    writes_b.lock().expect("应读取 B 写入").clear();

    let node_b = session_b.read_with(cx, |app, _| {
        app.runtime
            .document
            .nodes()
            .iter()
            .find(|node| matches!(node.kind, canvas::CanvasNodeKind::Agent(_)))
            .expect("B 应有 Agent 卡片")
            .id
    });
    let stale = host.update(cx, |app, cx| {
        app.norma_assign_workspace_task("已切走的工作空间", &area_b.id, node_b, "检查登录方案", cx)
    });
    assert!(stale.is_err(), "旧工作空间 ID 必须拒绝派活");
    assert!(
        writes_b.lock().expect("应读取 B 写入").is_empty(),
        "拒绝过期目标时不能写入 B 的 PTY"
    );
    let accepted = host.update(cx, |app, cx| {
        app.norma_assign_workspace_task(&workspace.id, &area_b.id, node_b, "检查登录方案", cx)
            .expect("稳定 ID 指向 B 时应提交")
    });
    assert_eq!(accepted.status, "accepted", "成功派活必须给出真实接收回执");
    assert_eq!(
        accepted.node_id,
        Some(node_b),
        "回执应携带实际 Agent 卡片 ID"
    );
    assert!(
        writes_a.lock().expect("应读取 A 写入").is_empty(),
        "跨项目工具不能写入当前聚焦的 A"
    );
    assert_eq!(
        writes_b
            .lock()
            .expect("应读取 B 写入")
            .iter()
            .filter(|chunk| String::from_utf8_lossy(chunk).contains("检查登录方案"))
            .count(),
        1,
        "跨项目工具只应向 B 发送一次任务正文"
    );
    writes_b.lock().expect("应读取 B 写入").clear();

    let node_a = session_a.read_with(cx, |app, _| {
        app.runtime
            .document
            .nodes()
            .iter()
            .find(|node| matches!(node.kind, canvas::CanvasNodeKind::Agent(_)))
            .expect("A 应有 Agent 卡片")
            .id
    });
    let reply_a = host.update(cx, |app, cx| {
        app.norma_assign_workspace_task(&workspace.id, &area_a.id, node_a, "修复注册问题", cx)
            .expect("A 的稳定 ID 应可派活")
    });
    let reply_b = host.update(cx, |app, cx| {
        app.norma_assign_workspace_task(&workspace.id, &area_b.id, node_b, "测试登录", cx)
            .expect("B 的稳定 ID 应可派活")
    });
    assert_eq!(reply_a.node_id, Some(node_a), "A 的回执应指向 A 卡片");
    assert_eq!(reply_b.node_id, Some(node_b), "B 的回执应指向 B 卡片");
    for (label, task, writes) in [
        ("A", "修复注册问题", &writes_a),
        ("B", "测试登录", &writes_b),
    ] {
        let writes = writes.lock().expect("应读取对应 PTY 写入");
        assert_eq!(
            writes
                .iter()
                .filter(|chunk| String::from_utf8_lossy(chunk).contains(task))
                .count(),
            1,
            "{label} 应只收到自己的一次任务"
        );
        assert_eq!(
            writes.iter().filter(|chunk| chunk.contains(&b'\r')).count(),
            1,
            "{label} 的任务应只提交一次 Enter"
        );
    }
    host.read_with(cx, |app, _| {
        let receipts = app
            .desktop_workspace
            .dispatch_receipts
            .get(&workspace.id)
            .expect("工作空间应保留逐目标回执");
        assert_eq!(receipts.len(), 3, "三次成功派活应各有独立回执");
        assert!(
            receipts.iter().all(|receipt| receipt.submitted),
            "各次成功派活均应有成功标记"
        );
    });

    session_a.update(cx, |app, _| app.runtime.kill_all());
    writes_a.lock().expect("应清空 A 写入").clear();
    writes_b.lock().expect("应清空 B 写入").clear();
    let failed_a = host.update(cx, |app, cx| {
        app.norma_assign_workspace_task(&workspace.id, &area_a.id, node_a, "修复支付问题", cx)
    });
    assert!(failed_a.is_err(), "停止的 A 应返回失败回执");
    let accepted_b = host.update(cx, |app, cx| {
        app.norma_assign_workspace_task(&workspace.id, &area_b.id, node_b, "测试支付", cx)
            .expect("A 失败不应影响 B")
    });
    assert_eq!(accepted_b.status, "accepted", "B 应独立提交成功");
    host.read_with(cx, |app, _| {
        let receipts = app
            .desktop_workspace
            .dispatch_receipts
            .get(&workspace.id)
            .expect("工作空间应保留成功与失败的逐目标回执");
        assert_eq!(receipts.len(), 5, "两次独立调用都应留下回执");
        assert_eq!(
            receipts[3].project_name, "ProjectA",
            "失败回执应保留 A 来源"
        );
        assert!(!receipts[3].submitted, "A 停止后应记录失败");
        assert!(receipts[3].status.contains("失败"), "失败原因应可见");
        assert_eq!(
            receipts[4].project_name, "ProjectB",
            "成功回执应保留 B 来源"
        );
        assert!(receipts[4].submitted, "B 应保留独立成功回执");
    });
    assert!(
        writes_a.lock().expect("应读取失败目标写入").is_empty(),
        "停止的 A 不能收到任务"
    );
    assert_eq!(
        writes_b
            .lock()
            .expect("应读取成功目标写入")
            .iter()
            .filter(|chunk| String::from_utf8_lossy(chunk).contains("测试支付"))
            .count(),
        1,
        "A 失败后 B 仍只能收到一次任务"
    );
    let counts = (
        writes_a.lock().expect("应统计 A 写入").len(),
        writes_b.lock().expect("应统计 B 写入").len(),
    );
    let stale_node = host.update(cx, |app, cx| {
        app.norma_assign_workspace_task(&workspace.id, &area_b.id, u64::MAX, "不应提交", cx)
    });
    assert!(stale_node.is_err(), "失效 Agent 节点应被拒绝");
    assert_eq!(
        writes_a.lock().expect("应统计 A 写入").len(),
        counts.0,
        "拒绝失效节点不得写入 A"
    );
    assert_eq!(
        writes_b.lock().expect("应统计 B 写入").len(),
        counts.1,
        "拒绝失效节点不得写入 B"
    );
    session_b.update(cx, |app, _| app.runtime.kill_all());
}

/// Norma 定位工具必须先校验卡片与待审请求，再改变中央项目画布。
///
/// 数据构造：工作空间含项目 B，已加载一张 Codex 卡片，当前仍在空间总览。
/// 执行过程：用过期审批回合尝试定位，再用同一张卡片的稳定 ID 普通定位。
/// 预期结果：过期审批拒绝且不切项目；普通定位展开 B 并选中原卡片。
#[test]
fn workspace_agent_locator_rejects_stale_approval_then_focuses_exact_card() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("ProjectB");
    std::fs::create_dir(&root).expect("应创建 B 项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Dispatch").expect("应创建桌面空间");
    let area = catalog
        .add_project(&workspace.id, &root)
        .expect("应添加 B 项目");
    let prepared =
        PreparedProject::load_for_desktop(paths.clone(), area.clone()).expect("应准备 B 项目实例");
    let mut test = TestAppContext::single();
    let (host, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    let session = cx.new(|cx| {
        let mut app = MultisoulApp::from_background_project(prepared, cx);
        app.runtime.set_pty_factory(Box::new(|_, _, _, tx| {
            Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
        }));
        app
    });
    host.update(cx, |app, _| {
        app.screen = AppScreen::DesktopWorkspace;
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.workspaces = vec![workspace.clone()];
        app.desktop_workspace.areas = vec![area.clone()];
        app.desktop_workspace
            .sessions
            .insert((workspace.id.clone(), area.id.clone()), session.clone());
    });
    session.update_in(cx, |app, window, cx| {
        app.spawn_agent_card(canvas::AgentProfile::Codex, window, cx);
    });
    cx.run_until_parked();
    let node_id = session.read_with(cx, |app, _| {
        app.runtime
            .document
            .nodes()
            .iter()
            .find(|node| matches!(node.kind, canvas::CanvasNodeKind::Agent(_)))
            .expect("B 应有 Agent 卡片")
            .id
    });
    let stale = host.update_in(cx, |app, window, cx| {
        app.norma_locate_workspace_agent(
            &workspace.id,
            &area.id,
            node_id,
            Some("old-run"),
            Some("old-turn"),
            window,
            cx,
        )
    });
    assert!(stale.is_err(), "非当前审批请求必须拒绝定位");
    host.read_with(cx, |app, _| {
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "拒绝过期审批不得切换中央画布"
        );
    });
    let unavailable = host.update_in(cx, |app, window, cx| {
        app.desktop_workspace.area_errors.insert(
            area.id.clone(),
            ui::i18n::LocalizedText::new("workspace.project_directory_is_still_unavailable"),
        );
        app.norma_locate_workspace_agent(&workspace.id, &area.id, node_id, None, None, window, cx)
    });
    assert!(unavailable.is_err(), "目录不可用时定位不得谎报成功");
    host.update(cx, |app, _| {
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "失败后仍应停在总览"
        );
        app.desktop_workspace.area_errors.remove(&area.id);
    });
    let accepted = host.update_in(cx, |app, window, cx| {
        app.norma_locate_workspace_agent(&workspace.id, &area.id, node_id, None, None, window, cx)
            .expect("有效 Agent 卡片应可定位")
    });
    assert_eq!(accepted.node_id, Some(node_id), "定位回执应指向原卡片");
    host.read_with(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_deref(),
            Some(area.id.as_str()),
            "中央应展开 B 项目"
        );
    });
    session.read_with(cx, |app, _| {
        assert!(app.runtime.selection.contains(node_id), "原卡片应选中");
    });
    session.update(cx, |app, _| app.runtime.kill_all());
}
