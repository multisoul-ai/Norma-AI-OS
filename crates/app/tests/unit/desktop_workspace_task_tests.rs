use super::*;
use crate::app::background_project::PreparedProject;
use crate::app::AppScreen;
use gpui::{AppContext as _, TestAppContext};
use std::sync::{Arc, Mutex};

/// 稳定 ID 派活工具必须进入指定项目的协调队列，并留下逐目标回执。
///
/// 数据构造：一个桌面空间含 ProjectA 和 ProjectB，各有一张 Codex 卡片；
/// 两个隔离 Recording PTY 分别收集写入，当前聚焦 ProjectA。
/// 执行过程：拒绝错误工作空间后向 B 派活，再分别向 A/B 派活；
/// 最后用无效节点 ID 验证过期目标，再从 A 项目会话向自己的 Agent 派活。
/// 预期结果：A 只收到自己的任务，B 的后续任务排队且不写入正在忙碌的 PTY；
/// Enter 留给协调队列；
/// 逐目标回执准确，过期目标不得产生 PTY 写入。
#[test]
fn workspace_task_tool_targets_exact_agent_through_coordinator() {
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
    let desktop_host = host.downgrade();
    let make_session = |prepared: PreparedProject,
                        writes: Arc<Mutex<Vec<Vec<u8>>>>,
                        cx: &mut gpui::VisualTestContext| {
        cx.new(|cx| {
            let mut app = MultisoulApp::from_background_project(prepared, cx);
            app.desktop_host = Some(desktop_host.clone());
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
        accepted.task_id.is_some(),
        "成功派活必须返回可追踪的任务 ID"
    );
    session_b.read_with(cx, |app, _| {
        assert!(
            app.agent_task_coordinator
                .as_ref()
                .is_some_and(|coordinator| coordinator.tasks().iter().any(|task| {
                    task.target_node_id == node_b && task.task == "检查登录方案"
                })),
            "跨项目派活必须进入目标项目的协调队列，供待审批保护与产物追踪使用"
        );
    });
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
    let writes_a_guard = writes_a.lock().expect("应读取 A 的 PTY 写入");
    assert_eq!(
        writes_a_guard
            .iter()
            .filter(|chunk| String::from_utf8_lossy(chunk).contains("修复注册问题"))
            .count(),
        1,
        "A 应只收到自己的任务"
    );
    assert_eq!(
        writes_a_guard
            .iter()
            .filter(|chunk| chunk.contains(&b'\r'))
            .count(),
        0,
        "A 的 Enter 应由协调队列在输入就绪后提交"
    );
    drop(writes_a_guard);
    assert!(
        writes_b.lock().expect("应读取 B 的 PTY 写入").is_empty(),
        "B 正在处理上一任务时，后续任务不得抢写 PTY"
    );
    session_b.read_with(cx, |app, _| {
        assert!(
            app.agent_task_coordinator
                .as_ref()
                .is_some_and(|coordinator| coordinator.tasks().iter().any(|task| {
                    task.target_node_id == node_b
                        && task.task == "测试登录"
                        && task.state == "queued"
                })),
            "B 的后续任务应保留在协调队列中"
        );
    });

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
    let same_project = session_a.update(cx, |app, cx| {
        app.norma_assign_workspace_task(&workspace.id, &area_a.id, node_a, "补充注册验收", cx)
            .expect("项目会话派给自身 Agent 不应重入读取同一实体")
    });
    assert!(
        same_project.task_id.is_some(),
        "同项目派活也应返回可追踪任务 ID"
    );
    // 场景：A 项目会话请求定位 B 的 Agent，导航需要先分离仍在更新的 A。
    // 数据构造：A 是当前项目，B 的 Codex 卡片已经加载。
    // 执行过程：从 A 的 update 回调发起定位，随后执行延后导航。
    // 预期结果：不重入 A；画布切到 B，且 B 的目标卡片获得选择。
    let cross_project = session_a.update_in(cx, |app, window, cx| {
        app.norma_locate_workspace_agent(&workspace.id, &area_b.id, node_b, None, None, window, cx)
            .expect("跨项目定位不应在来源项目更新期间导航")
    });
    assert_eq!(
        cross_project.node_id,
        Some(node_b),
        "跨项目定位回执应指向 B 的卡片"
    );
    cx.run_until_parked();
    host.read_with(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_deref(),
            Some(area_b.id.as_str()),
            "延后导航必须把中央画布切到 B"
        );
    });
    session_b.read_with(cx, |app, _| {
        assert!(app.runtime.selection.contains(node_b), "B 的目标卡片应选中");
    });
    session_a.update(cx, |app, _| app.runtime.kill_all());
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
    let desktop_host = host.downgrade();
    let session = cx.new(|cx| {
        let mut app = MultisoulApp::from_background_project(prepared, cx);
        app.desktop_host = Some(desktop_host);
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
    session.update(cx, |app, cx| {
        app.runtime.selection.clear();
        cx.notify();
    });
    session.read_with(cx, |app, _| {
        assert!(
            !app.runtime.selection.contains(node_id),
            "新定位前先清除旧选择"
        );
    });
    // 场景：Norma 从项目会话定位本项目的 Agent。
    // 数据构造：当前展开项目就是命令来源，目标仍是同一张 Codex 卡片。
    // 执行过程：在项目实体的 update 回调内调用真实定位工具，再执行延后导航。
    // 预期结果：工具不重入读取或更新项目；最终仍选中目标卡片。
    let from_project = session.update_in(cx, |app, window, cx| {
        app.norma_locate_workspace_agent(&workspace.id, &area.id, node_id, None, None, window, cx)
            .expect("项目会话定位自身 Agent 不应重入")
    });
    assert_eq!(
        from_project.node_id,
        Some(node_id),
        "项目会话定位回执应指向原卡片"
    );
    cx.run_until_parked();
    session.read_with(cx, |app, _| {
        assert!(
            app.runtime.selection.contains(node_id),
            "延后导航后原卡片应选中"
        );
    });
    session.update(cx, |app, _| app.runtime.kill_all());
}
