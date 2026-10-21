use super::*;
use crate::app::background_project::PreparedProject;
use gpui::{AppContext as _, TestAppContext};
use std::sync::{Arc, Mutex};
use workspace::ideas::IdeaState;

/// 想法工具经真实适配器：记录、跨项目派发、回滚与重复派发保护。
///
/// 数据构造：桌面空间含 ProjectA（聚焦，Norma 所在会话）与 ProjectB，各一张 Codex 卡片，
/// 两个 Recording PTY 收集写入；先记两条想法“检查登录方案”“测试注册流程”。
/// 执行过程：
///   1. 从 A 会话把第一条派给 B 的 Agent；再次派发同一条。
///   2. 把第二条派给不存在的节点 9999；再派给错误的工作空间。
///   3. 把第二条以“新建智能体”派到非当前区域 B。
/// 预期结果：
///   - 正断言：第一次回执 accepted，想法变为已指派并记录 B 的区域与节点，task_ref 为协调器里的真实值；B 的 PTY 恰好收到一次正文。
///   - 负断言：重复派发被拒绝且 PTY 不再写入；无效节点与新建到非当前区域失败后第二条想法都回到待办且 task_ref 被清除；错误工作空间被拒绝且想法不进入派发中。
#[test]
fn ideas_capture_assign_rollback_and_reject_duplicates() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root_a = temp.path().join("ProjectA");
    let root_b = temp.path().join("ProjectB");
    std::fs::create_dir(&root_a).expect("应创建 A 项目目录");
    std::fs::create_dir(&root_b).expect("应创建 B 项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Ideas").expect("应创建桌面空间");
    let area_a = catalog
        .add_project(&workspace.id, &root_a)
        .expect("应添加 A 项目");
    let area_b = catalog
        .add_project(&workspace.id, &root_b)
        .expect("应添加 B 项目");
    let prepared_a =
        PreparedProject::load_for_desktop(paths.clone(), area_a.clone()).expect("应准备 A");
    let prepared_b =
        PreparedProject::load_for_desktop(paths.clone(), area_b.clone()).expect("应准备 B");
    let ideas = IdeasStore::new(paths.app_db_path().to_owned());
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

    let captured = session_a
        .update(cx, |app, cx| {
            capture_idea(app, cx, "检查登录方案", AgentInputSource::Other)
        })
        .expect("应记下第一条想法");
    session_a
        .update(cx, |app, cx| {
            capture_idea(app, cx, "测试注册流程", AgentInputSource::GptLiveVoice)
        })
        .expect("应记下第二条想法");
    let listed = session_a
        .update(cx, |app, cx| list_ideas(app, cx))
        .expect("应列出想法");
    assert_eq!(listed.tasks.len(), 2, "清单应有两条想法");
    let first_id = captured.tasks[0]["idea_id"]
        .as_str()
        .expect("回执应带 idea_id")
        .to_owned();
    let second_id = listed.tasks[1]["idea_id"]
        .as_str()
        .expect("第二条应有 idea_id")
        .to_owned();
    let find = |id: &str| {
        ideas
            .list(&workspace.id)
            .expect("应列出")
            .into_iter()
            .find(|idea| idea.id == id)
            .expect("想法应存在")
    };
    assert_eq!(
        find(&second_id).source,
        IdeaSource::Voice,
        "语音来源的想法应记为 Voice"
    );

    let to_b = IdeaTarget::Agent {
        workspace_id: workspace.id.clone(),
        area_id: area_b.id.clone(),
        node_id: node_b,
    };
    let accepted = session_a
        .update(cx, |app, cx| {
            assign_idea(
                app,
                None,
                cx,
                &first_id,
                to_b.clone(),
                AgentInputSource::Other,
            )
        })
        .expect("待办想法应可派给 B 的 Agent");
    assert_eq!(accepted.status, "accepted", "派发应给出真实接收回执");
    let real_ref = session_b.read_with(cx, |app, _| {
        app.agent_task_coordinator
            .as_ref()
            .and_then(|coordinator| {
                coordinator
                    .tasks()
                    .into_iter()
                    .find(|task| task.task == "检查登录方案")
            })
            .map(|task| task.task_ref)
            .expect("B 的协调器应有这条任务")
    });
    let assigned = find(&first_id);
    assert_eq!(
        assigned.state,
        IdeaState::Assigned,
        "派发成功后想法应为已指派"
    );
    assert_eq!(
        (
            assigned.assignee_area_id.as_deref(),
            assigned.assignee_node_id,
            assigned.task_ref.as_deref()
        ),
        (
            Some(area_b.id.as_str()),
            Some(node_b),
            Some(real_ref.as_str())
        ),
        "应记录 B 的区域、节点与协调器里的真实 task_ref"
    );
    let writes_count = |writes: &Arc<Mutex<Vec<Vec<u8>>>>| {
        writes
            .lock()
            .expect("应读取写入")
            .iter()
            .filter(|chunk| String::from_utf8_lossy(chunk).contains("检查登录方案"))
            .count()
    };
    assert_eq!(writes_count(&writes_b), 1, "B 的 PTY 应恰好收到一次正文");

    let again = session_a.update(cx, |app, cx| {
        assign_idea(
            app,
            None,
            cx,
            &first_id,
            to_b.clone(),
            AgentInputSource::Other,
        )
    });
    assert!(
        matches!(again, Err(AdapterError::Rejected(_))),
        "已指派的想法不能再次派发"
    );
    assert_eq!(writes_count(&writes_b), 1, "重复派发不应再写入 PTY");

    let stale = IdeaTarget::Agent {
        workspace_id: workspace.id.clone(),
        area_id: area_b.id.clone(),
        node_id: 9999,
    };
    let result = session_a.update(cx, |app, cx| {
        assign_idea(app, None, cx, &second_id, stale, AgentInputSource::Other)
    });
    assert!(result.is_err(), "无效节点必须失败");
    assert_eq!(
        (find(&second_id).state, find(&second_id).task_ref),
        (IdeaState::Todo, None),
        "派发失败后想法应回到待办并清除 task_ref"
    );

    let wrong_workspace = IdeaTarget::Agent {
        workspace_id: "other".into(),
        area_id: area_b.id.clone(),
        node_id: node_b,
    };
    let result = session_a.update(cx, |app, cx| {
        assign_idea(
            app,
            None,
            cx,
            &second_id,
            wrong_workspace,
            AgentInputSource::Other,
        )
    });
    assert!(
        matches!(result, Err(AdapterError::Rejected(_))),
        "错误的工作空间必须拒绝"
    );
    assert_eq!(
        find(&second_id).state,
        IdeaState::Todo,
        "被拒绝的派发不应让想法进入派发中"
    );

    let new_in_b = IdeaTarget::NewAgent {
        workspace_id: workspace.id.clone(),
        area_id: area_b.id.clone(),
        profile: norma::protocol::AgentProfile::Codex,
    };
    let result = session_a.update_in(cx, |app, window, cx| {
        assign_idea(
            app,
            Some(window),
            cx,
            &second_id,
            new_in_b,
            AgentInputSource::Other,
        )
    });
    assert!(
        matches!(result, Err(AdapterError::Rejected(_))),
        "新建智能体暂不支持非当前区域"
    );
    assert_eq!(
        (find(&second_id).state, find(&second_id).task_ref),
        (IdeaState::Todo, None),
        "被拒绝后想法应回到待办"
    );
}
