use super::*;
use canvas::{AgentProfile, AgentSpec, CanvasNodeKind, CanvasRect};
use gpui::{AppContext, EmptyView, TestAppContext};
use norma::protocol::{Worker, WorkerOutcome, WorkerPhase};

/// 场景：从桌面任务页移除并关闭一张已经结束的复用 Worker 卡片。
/// 数据构造：隔离任务数据库和双向关联的桌面宿主、项目；Bruno 两次派发共享一张卡，
/// Ethan 的一次派发使用另一张卡。派发共 2 + 1 条且均已结束，卡片均为 320×240 的测试占位。
/// 执行过程：宿主把关闭请求交给项目；项目清空确认框，异步保存移除状态，再关闭画布节点。
/// 预期结果：确认框消失，只移除后一次 Bruno 派发并关闭 Bruno 的卡，历史派发及 Ethan 保留。
#[test]
fn staff_worker_remove_close_preserves_other_cards_and_dispatches() {
    let dir = tempfile::tempdir().expect("isolated data");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut test = TestAppContext::single();
    let (_, cx) = test.add_window_view(|_, _| EmptyView);
    let host = cx.new(|cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths.clone(),
        )
    });
    let project = cx.new(|cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    let (store, bruno, ethan) = project.update(cx, |app, _| {
        app.desktop_host = Some(host.downgrade());
        app.workspace_id = Some("workspace".into());
        app.workspace_route_id = Some("route".into());
        let store = app.install_staff_store_for_test(dir.path());
        let mut add_card = |name: &str| {
            app.runtime.document.add_node(
                CanvasNodeKind::Agent(AgentSpec {
                    profile: AgentProfile::Codex,
                    alias: Some(name.into()),
                    definition_id: None,
                    mobile_control: false,
                }),
                CanvasRect::new(0.0, 0.0, 320.0, 240.0),
            )
        };
        let bruno = add_card("Bruno");
        let ethan = add_card("Ethan");
        let mut task = Task::new("task", "workspace", "project", "Worker removal");
        let mut first = Worker::new("Bruno", "Codex", "Build", bruno, "first");
        first.phase = WorkerPhase::Finished(WorkerOutcome::Succeeded);
        let mut reused = first.clone();
        reused.dispatch_id = "reused".into();
        reused.reused = true;
        let mut other = Worker::new("Ethan", "Codex", "Build", ethan, "other");
        other.phase = WorkerPhase::Finished(WorkerOutcome::Succeeded);
        task.workers = vec![first, reused, other];
        store.create(task.clone()).expect("isolated task persisted");
        app.staff.tasks.insert(task.id.clone(), task);
        (store, bruno, ethan)
    });
    host.update_in(cx, |app, window, cx| {
        app.screen = crate::app::AppScreen::DesktopWorkspace;
        app.desktop_workspace.active_id = Some("workspace".into());
        app.desktop_workspace
            .sessions
            .insert(("workspace".into(), "project".into()), project.clone());
        app.staff
            .task_projects
            .insert("task".into(), ("project".into(), "Project".into()));
        app.staff.remove_ask = Some(WorkerRemoveAsk {
            task_id: "task".into(),
            dispatch_id: "reused".into(),
            agent: "Bruno".into(),
        });
        app.request_remove_worker(
            RemoveRequest {
                task_id: "task".into(),
                dispatch_id: "reused".into(),
                close_card: true,
                confirmed: false,
            },
            window,
            cx,
        );
    });
    assert!(
        host.read_with(cx, |app, _| app.staff.remove_ask.is_none()),
        "已结束的 Worker 无需确认，宿主须清除旧确认框"
    );
    cx.run_until_parked();
    project.read_with(cx, |app, _| {
        assert!(app.staff.error.is_none(), "移除应通过真实任务写入流程");
        assert!(
            app.runtime.document.node(bruno).is_none(),
            "共享的 Bruno 卡片必须关闭"
        );
        assert!(
            app.runtime.document.node(ethan).is_some(),
            "Ethan 卡片必须保留"
        );
    });
    let saved = store.get("workspace", "task").expect("reopen task record");
    assert!(
        saved.worker_for_dispatch("reused").is_none(),
        "选中的复用派发必须移除"
    );
    assert!(
        saved.worker_for_dispatch("first").is_some(),
        "历史派发必须保留"
    );
    assert!(
        saved.worker_for_dispatch("other").is_some(),
        "Ethan 的派发必须保留"
    );
    assert_eq!(saved.removed_workers.len(), 1, "只能记录一次移除");
    assert_eq!(
        saved.removed_workers[0].worker.dispatch_id, "reused",
        "移除日志必须指向选中的派发"
    );
}
