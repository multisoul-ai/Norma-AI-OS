use super::*;
use crate::app::background_project::PreparedProject;
use agent::context::*;
use gpui::{AppContext as _, TestAppContext};

// The production pump prepares first, authenticates, then enters the exact-target
// synchronous executor. Exercise that same preparation, with no fixture prewarming.
fn execute_prepared(
    database: &Path,
    areas: &[WorkspaceArea],
    command: AdapterCommand,
    cx: &mut gpui::VisualTestContext,
) -> Result<AdapterResult, AdapterError> {
    let database = database.to_path_buf();
    let areas = areas.to_vec();
    let (sent, received) = std::sync::mpsc::channel();
    cx.update(|_, cx| {
        cx.spawn(async move |cx| {
            let result = match prepare_scoped(&database, &areas, &command, cx).await {
                Ok(()) => cx
                    .update(|cx| execute_scoped(&database, &areas, "phone", "op", command, cx))
                    .unwrap_or_else(|error| Err(AdapterError::Failed(error.to_string()))),
                Err(error) => Err(error),
            };
            sent.send(result).unwrap();
        })
        .detach()
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        if let Ok(result) = received.try_recv() {
            return result;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "群聊后台准备应在期限内完成"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// 场景：手机群聊模型首次向冷项目派发；之后用户暂停派发也不能改用 reply 绕过。
/// 构造：隔离桌面空间、一个后台项目及真实协调器 Agent；仅在首派之后保存限制。
/// 执行：正式 execute_scoped 首派，再分别以 assign/reply 请求同一目标。
/// 预期：首次返回真实 task_id；限制后两种模型入口均拒绝且任务数不增加。
#[test]
fn workspace_context_prepares_cold_dispatch_and_guards_model_replies() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let database = paths.app_db_path().to_path_buf();
    let store = storage::AppStore::initialize(&paths).unwrap();
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Mobile").unwrap();
    let area = catalog.add_project(&workspace.id, dir.path()).unwrap();
    let prepared = PreparedProject::load_for_desktop(paths.clone(), area.clone()).unwrap();
    let mut test = TestAppContext::single();
    let (host, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    let session = cx.new(|cx| {
        let mut app = MultisoulApp::from_background_project(prepared, cx);
        app.desktop_host = Some(host.downgrade());
        app.runtime.set_pty_factory(Box::new(|_, _, _, tx| {
            Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
        }));
        app
    });
    host.update_in(cx, |app, window, cx| {
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.areas = vec![area.clone()];
        app.desktop_workspace
            .sessions
            .insert((workspace.id.clone(), area.id.clone()), session.clone());
        crate::workspace::desktop::sessions::register(
            database.clone(),
            window.window_handle(),
            cx.entity(),
            cx,
        );
    });
    session.update_in(cx, |app, window, cx| {
        app.spawn_agent_card(canvas::AgentProfile::Codex, window, cx);
    });
    cx.run_until_parked();
    let node = session.read_with(cx, |app, _| {
        app.runtime
            .document
            .nodes()
            .iter()
            .find(|n| matches!(n.kind, canvas::CanvasNodeKind::Agent(_)))
            .unwrap()
            .id
    });
    let areas = vec![WorkspaceArea {
        area: area.clone(),
        project_id: "project".into(),
    }];
    let address = format!("{}/{node}", area.id);
    assert!(
        crate::app::doubao_adapter_fixture::is_cold(&session, cx),
        "首次群聊派发不能预热目标"
    );
    let first = execute_prepared(
        &database,
        &areas,
        AdapterCommand::AssignAgentTask {
            agent: address.clone(),
            task: "检查项目".into(),
        },
        cx,
    )
    .expect("群聊冷目标首次派发应自动加载共享限制");
    assert!(first.task_id.is_some(), "必须真实进入任务协调器而非空回执");
    let service = session.update(cx, |app, _| app.shared_context_service().unwrap());
    crate::app::shared_tokio_runtime()
        .unwrap()
        .block_on(service.run(Some("pause".into()), |access| {
            let input = access.record_input("t1", ContextOrigin::Voice, "不要派发任务")?;
            access.update(
                "u1",
                &input.reference,
                &ContextStateUpdate {
                    input: input.reference.clone(),
                    expected_revision: input.reference.revision,
                    change: ContextStateChange::Constraint {
                        key: "pause".into(),
                        text: "不要派发任务".into(),
                        supersedes: None,
                        enforcement: ConstraintEnforcement::Enforced {
                            operation: ContextOperation::DispatchTask,
                            target: None,
                        },
                    },
                },
            )
        }))
        .unwrap();
    for command in [
        AdapterCommand::AssignAgentTask {
            agent: address.clone(),
            task: "继续".into(),
        },
        AdapterCommand::ReplyAgent {
            agent: address.clone(),
            message: "继续".into(),
        },
    ] {
        let error = execute_prepared(&database, &areas, command, cx).unwrap_err();
        assert!(
            error.to_string().contains("context_operation_restricted"),
            "群聊模型不能借 reply 绕过暂停：{error}"
        );
    }
    session.read_with(cx, |app, _| {
        assert_eq!(
            app.agent_task_coordinator.as_ref().unwrap().tasks().len(),
            1,
            "只允许首次真实派发，拒绝不增加任务"
        );
    });
}
