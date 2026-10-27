use super::*;
use crate::app::background_project::PreparedProject;
use gpui::{AppContext as _, TestAppContext};

/// 场景：工作空间请求完成异步准备后才被撤销，不得继续产生新的派工副作用。
/// 构造：真实隔离工作空间、Agent 卡片和任务协调器；加载共享状态后进入生产最终执行入口。
/// 执行：正常派发一次，再分别取消回合或运行时后尝试相同入口派发。
/// 预期：正常请求产生 task_id；撤销请求返回 Cancelled，原已接受任务保留且任务数始终为一。
#[test]
fn workspace_memory_revocation_blocks_prepared_dispatch_without_rolling_back_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let database = paths.app_db_path().to_owned();
    let store = storage::AppStore::initialize(&paths).unwrap();
    let catalog = store.desktop_workspace_store();
    let space = catalog.create("Cancel").unwrap();
    let area = catalog.add_project(&space.id, dir.path()).unwrap();
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
        app.desktop_workspace.active_id = Some(space.id.clone());
        app.desktop_workspace.areas = vec![area.clone()];
        app.desktop_workspace
            .sessions
            .insert((space.id, area.id.clone()), session.clone());
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
    let service = session.update(cx, |app, _| app.shared_context_service().unwrap());
    shared_tokio_runtime()
        .unwrap()
        .block_on(service.snapshot())
        .unwrap();
    let scope = Scope {
        areas: vec![WorkspaceArea {
            area: area.clone(),
            project_id: "project".into(),
        }],
        device: "phone".into(),
        token: "fixture".into(),
        operation: "dispatch".into(),
    };
    let command = || AdapterCommand::AssignAgentTask {
        agent: format!("{}/{node}", area.id),
        task: "检查项目".into(),
    };
    let receipt = cx
        .update(|_, cx| {
            execute_ready(
                &database,
                &scope,
                command(),
                &CancellationToken::new(),
                &CancellationToken::new(),
                cx,
            )
        })
        .unwrap();
    assert!(receipt.task_id.is_some(), "正常入口必须产生真实任务回执");
    for cancel_runtime in [false, true] {
        let lifetime = CancellationToken::new();
        let turn = CancellationToken::new();
        if cancel_runtime {
            lifetime.cancel();
        } else {
            turn.cancel();
        }
        let result =
            cx.update(|_, cx| execute_ready(&database, &scope, command(), &lifetime, &turn, cx));
        assert!(
            matches!(result, Err(AdapterError::Cancelled)),
            "准备后撤销必须阻止派发：{result:?}"
        );
        session.read_with(cx, |app, _| {
            assert_eq!(
                app.agent_task_coordinator.as_ref().unwrap().tasks().len(),
                1,
                "撤销不能追加任务，也不能回滚已经接受的任务"
            );
        });
    }
}
