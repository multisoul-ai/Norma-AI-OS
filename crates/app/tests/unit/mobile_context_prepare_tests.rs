use super::*;
use crate::app::MultisoulApp;
use agent::{AdapterCommand, AgentInputSource};
use gpui::TestAppContext;
use std::sync::Mutex;

/// 场景：手机实时工具第一次向已打开项目输入，尚未创建共享上下文服务。
/// 构造：独立数据库与正式路由、一张记录输入的 PTY、有效执行 lease。
/// 执行：通过正式 mobile_command::execute 提交，在 GPUI 推进后台准备。
/// 预期：冷项目首次输入实际到达 PTY；不能因状态未加载返回 outcome_unknown。
#[test]
fn mobile_context_prepares_cold_realtime_tool_before_acceptance() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let store = storage::AppStore::initialize(&paths).unwrap();
    let database = paths.app_db_path().to_path_buf();
    let mut test = TestAppContext::single();
    let (app, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    let writes = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let recording = writes.clone();
    let (workspace, route, node) = app.update_in(cx, |app, window, cx| {
        crate::norma::endpoint::install_at_path(dir.path().join("norma.sock"), cx).unwrap();
        app.runtime.set_pty_factory(Box::new(move |_, _, _, tx| {
            Ok(Box::new(terminal::TestPtyBackend::recording(
                tx,
                recording.clone(),
            )))
        }));
        app.enter_canvas_for_test(dir.path(), cx);
        app.register_loaded_project_route(None, cx);
        let created = crate::app::execute_norma_adapter_command_with_source(
            app,
            Some(window),
            cx,
            AdapterCommand::CreateTerminal,
            AgentInputSource::Voice,
        )
        .unwrap();
        (
            app.workspace_id.clone().unwrap(),
            app.workspace_route_id.clone().unwrap(),
            created.node_id.unwrap(),
        )
    });
    assert!(
        crate::app::doubao_adapter_fixture::is_cold(&app, cx),
        "夹具不能预热共享状态"
    );
    let source = NewMobileCommand {
        device_id: "phone".into(),
        operation_id: "input-1".into(),
        workspace_id: workspace.clone(),
        route_id: route.clone(),
        owner_id: "owner".into(),
        request_hash: "hash".into(),
    };
    let (reply, receipt) = crossbeam_channel::bounded(1);
    cx.update(|_, cx| {
        crate::norma::endpoint::mobile_command::execute(
            database,
            LiveCommandRequest {
                source,
                request: MobileCommandRequest {
                    operation_id: "input-1".into(),
                    workspace_id: workspace,
                    route_id: route,
                    grant_revision: 1,
                    command: MobileCommand::RealtimeTool {
                        command: AdapterCommand::SendTerminalInput {
                            node_id: node,
                            text: "mobile-cold-input".into(),
                            submit: false,
                        },
                    },
                },
                lease: Arc::new(CommandLease::new("input-1")),
                deadline: Instant::now() + Duration::from_secs(5),
                reply,
            },
            cx,
        )
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        if writes
            .lock()
            .unwrap()
            .concat()
            .windows(b"mobile-cold-input".len())
            .any(|part| part == b"mobile-cold-input")
        {
            break;
        }
        if let Ok(result) = receipt.try_recv() {
            panic!("冷项目输入尚未发送，不能提前返回：{result:?}");
        }
        assert!(Instant::now() < deadline, "首次输入必须在期限内被实际接收");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        writes.lock().unwrap().concat(),
        b"mobile-cold-input",
        "准备阶段不能重复发送输入或追加 Enter"
    );
}
