use super::*;
use crate::app::background_project::PreparedProject;
use crate::app::AppScreen;
use gpui::{AppContext as _, TestAppContext};
use std::sync::{Arc, Mutex};
use workspace::ideas::IdeaState;

/// 整条用户流程：手动添加想法 → 清单里出现 → 派发给另一项目的智能体 → 数据、终端写入与流星都就位。
///
/// 数据构造：桌面空间含 ProjectA（展开）与 ProjectB，B 有一张 Codex 卡片并带 Recording PTY；
///   在想法输入框里写“手动写下的想法”。
/// 执行过程：
///   1. 调用 ensure_idea_input 建立真实输入框，写入文字并触发添加；
///   2. 等后台写库与刷新完成，读取宿主缓存；
///   3. 以界面入口 dispatch_idea 派发到 B 的卡片，松手点 (30,40)；等延后的派发与刷新完成。
/// 预期结果：
///   - 正断言：缓存与库里都有一条来源为 Manual、正文完全一致的待办；派发后变为已指派，记录 B 的区域与节点，task_ref 非占位；
///     B 的 PTY 恰好收到一次正文；B 会话里有起点为 IdeaRow(30,40) 的流星；输入框已清空。
///   - 负断言：空白输入不产生想法；派发后清单里没有残留的待办。
#[test]
fn manual_idea_is_added_listed_and_dispatched_end_to_end() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root_a = temp.path().join("ProjectA");
    let root_b = temp.path().join("ProjectB");
    std::fs::create_dir(&root_a).expect("应创建 A 项目目录");
    std::fs::create_dir(&root_b).expect("应创建 B 项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Flow").expect("应创建桌面空间");
    let area_a = catalog
        .add_project(&workspace.id, &root_a)
        .expect("应添加 A");
    let area_b = catalog
        .add_project(&workspace.id, &root_b)
        .expect("应添加 B");
    let prepared_a =
        PreparedProject::load_for_desktop(paths.clone(), area_a.clone()).expect("应准备 A");
    let prepared_b =
        PreparedProject::load_for_desktop(paths.clone(), area_b.clone()).expect("应准备 B");
    let ideas_db = storage::IdeasStore::new(paths.app_db_path().to_owned());
    let mut test = TestAppContext::single();
    let (host, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
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
    let session_a = make_session(prepared_a, Arc::new(Mutex::new(Vec::new())), cx);
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
    session_b.update_in(cx, |app, window, cx| {
        app.spawn_agent_card(canvas::AgentProfile::Codex, window, cx);
    });
    cx.run_until_parked();
    writes_b.lock().expect("应读取写入").clear();
    let node_b = session_b.read_with(cx, |app, _| {
        app.runtime
            .document
            .nodes()
            .iter()
            .find(|node| matches!(node.kind, canvas::CanvasNodeKind::Agent(_)))
            .expect("B 应有 Agent 卡片")
            .id
    });

    // 1. 手动添加：先试空白，再写正文。
    host.update_in(cx, |app, window, cx| {
        app.ensure_idea_input(window, cx);
        let input = app
            .desktop_workspace
            .ideas_input
            .clone()
            .expect("应建立输入框");
        input.update(cx, |state, cx| state.set_value("   ", window, cx));
        app.add_idea_from_input(window, cx);
    });
    cx.run_until_parked();
    assert!(
        ideas_db.list(&workspace.id).expect("应列出").is_empty(),
        "空白输入不应产生想法"
    );
    host.update_in(cx, |app, window, cx| {
        let input = app
            .desktop_workspace
            .ideas_input
            .clone()
            .expect("应有输入框");
        input.update(cx, |state, cx| {
            state.set_value("手动写下的想法", window, cx)
        });
        app.add_idea_from_input(window, cx);
    });
    cx.run_until_parked();
    let stored = ideas_db.list(&workspace.id).expect("应列出");
    assert_eq!(stored.len(), 1, "应记下一条想法");
    assert_eq!(
        (stored[0].body.as_str(), stored[0].source, stored[0].state),
        (
            "手动写下的想法",
            storage::IdeaSource::Manual,
            IdeaState::Todo
        ),
        "应是来源为手动的待办"
    );
    host.read_with(cx, |app, cx| {
        assert_eq!(
            app.desktop_workspace.ideas.len(),
            1,
            "宿主缓存应刷新出这条想法"
        );
        let input = app
            .desktop_workspace
            .ideas_input
            .clone()
            .expect("应有输入框");
        assert_eq!(input.read(cx).value().to_string(), "", "添加后输入框应清空");
    });

    // 2. 派发给 B 的智能体（界面入口，松手点 30,40）。
    let idea_id = stored[0].id.clone();
    host.update_in(cx, |app, window, cx| {
        app.dispatch_idea(
            idea_id.clone(),
            IdeaUiTarget::Existing {
                area_id: area_b.id.clone(),
                node_id: node_b,
            },
            Some((30.0, 40.0)),
            window,
            cx,
        );
    });
    cx.run_until_parked();
    cx.run_until_parked();
    let assigned = ideas_db.list(&workspace.id).expect("应列出").remove(0);
    assert_eq!(assigned.state, IdeaState::Assigned, "派发后应为已指派");
    assert_eq!(
        (
            assigned.assignee_area_id.as_deref(),
            assigned.assignee_node_id
        ),
        (Some(area_b.id.as_str()), Some(node_b)),
        "应记录 B 的区域与节点"
    );
    assert!(
        assigned
            .task_ref
            .as_deref()
            .is_some_and(|task_ref| !task_ref.starts_with("idea:")),
        "task_ref 应是协调器里的真实值，实际 {:?}",
        assigned.task_ref
    );
    let delivered = writes_b
        .lock()
        .expect("应读取写入")
        .iter()
        .filter(|chunk| String::from_utf8_lossy(chunk).contains("手动写下的想法"))
        .count();
    assert_eq!(delivered, 1, "B 的 PTY 应恰好收到一次正文");
    session_b.read_with(cx, |app, _| {
        let flights = app.motion.dispatch_receipt_presentations();
        assert!(
            flights.iter().any(|flight| flight.origin
                == crate::motion::DispatchVisualOrigin::IdeaRow { x: 30, y: 40 }
                && flight.target_node_id == node_b),
            "B 会话里应有从想法行(30,40)飞向 B 卡片的流星：{flights:?}"
        );
    });
    host.read_with(cx, |app, _| {
        assert!(
            app.desktop_workspace
                .ideas
                .iter()
                .all(|idea| idea.state != IdeaState::Todo),
            "缓存里不应残留待办"
        );
        assert!(
            app.desktop_workspace.ideas_notice.is_none(),
            "不应有派发失败提示：{:?}",
            app.desktop_workspace.ideas_notice
        );
    });
}
