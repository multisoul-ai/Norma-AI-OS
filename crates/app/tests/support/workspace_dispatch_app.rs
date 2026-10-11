use crate::app::{background_project::PreparedProject, MultisoulApp};
use gpui::{AppContext as _, TestAppContext};
use std::sync::{Arc, Mutex};

type Writes = Arc<Mutex<Vec<Vec<u8>>>>;

pub(crate) fn prepare_betty(
    host: &gpui::Entity<MultisoulApp>,
    cx: &mut gpui::VisualTestContext,
    service: &crate::workspace::dispatch::WorkspaceDispatchService,
    workspace: &str,
    operation: &str,
) -> crate::workspace::dispatch::PendingWorkspaceDispatch {
    prepare_request(host, cx, service, workspace, operation, "让 Betty 修复登录")
}

pub(crate) fn prepare_request(
    host: &gpui::Entity<MultisoulApp>,
    cx: &mut gpui::VisualTestContext,
    service: &crate::workspace::dispatch::WorkspaceDispatchService,
    workspace: &str,
    operation: &str,
    text: &str,
) -> crate::workspace::dispatch::PendingWorkspaceDispatch {
    use crate::workspace::dispatch::{WorkspaceDispatchEntry, WorkspaceDispatchPreparation};
    let source = norma::workspace_operation::WorkspaceOperationSource {
        workspace_id: workspace.into(),
        origin: norma::workspace_operation::WorkspaceOperationOrigin::Desktop {
            host_id: "host".into(),
        },
        operation_id: operation.into(),
        message_id: "message".into(),
        original_text: text.into(),
    };
    let output = Arc::new(Mutex::new(None));
    let capture = output.clone();
    let task = host.update(cx, |_, cx| {
        service.prepare(
            WorkspaceDispatchEntry::desktop(
                source,
                norma::workspace_dispatch::WorkspaceRoutingMode::Execute,
                vec![],
            )
            .expect("可信来源"),
            cx,
        )
    });
    cx.spawn(async move |_| {
        *capture.lock().expect("结果") = Some(task.await);
    })
    .detach();
    cx.run_until_parked();
    let prepared = output
        .lock()
        .expect("结果")
        .take()
        .expect("准备结束")
        .expect("准备成功");
    match prepared {
        WorkspaceDispatchPreparation::Ready(pending) => pending,
        _ => panic!("必须绑定原 Betty"),
    }
}
/// Fixtures keep the real shared service, desktop session registry and recorded PTYs intact.
pub(crate) fn with_workspace(
    run: impl FnOnce(
        gpui::Entity<MultisoulApp>,
        &mut gpui::VisualTestContext,
        storage::DatabasePaths,
        storage::DesktopWorkspace,
        storage::DesktopWorkspace,
        Vec<(storage::DesktopProjectArea, gpui::Entity<MultisoulApp>)>,
        [Writes; 2],
    ),
) {
    let temp = tempfile::tempdir().expect("隔离数据");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("AppDB");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("空间").expect("空间");
    let other = catalog.create("其他空间").expect("切换目标");
    let writes: [Arc<Mutex<Vec<Vec<u8>>>>; 2] =
        std::array::from_fn(|_| Arc::new(Mutex::new(Vec::new())));
    let mut fixtures = vec![];
    for (index, name) in ["Alan", "Betty"].into_iter().enumerate() {
        let root = temp.path().join(name);
        std::fs::create_dir(&root).expect("项目目录");
        let root = root.canonicalize().expect("canonical root");
        let area = catalog.add_project(&workspace.id, &root).expect("添加项目");
        let mut runtime = runtime::Runtime::new_for_workspace(&root);
        let record = writes[index].clone();
        runtime.set_pty_factory(Box::new(move |_, _, _, tx| {
            Ok(Box::new(terminal::TestPtyBackend::recording(
                tx,
                record.clone(),
            )))
        }));
        let node = runtime
            .spawn_agent(
                canvas::AgentProfile::OpenCode,
                canvas::CanvasRect::new(0., 0., 640., 400.),
                80,
                24,
            )
            .expect("既有卡片");
        let card = runtime
            .workspace_card_target(&workspace.id, &area.id, 0, node)
            .expect("原卡身份")
            .card;
        storage::ProjectStore::initialize_for_desktop_area(
            &paths,
            &area.root,
            &workspace.id,
            &area.id,
        )
        .expect("区域 DB")
        .canvas_store()
        .save_snapshot(&runtime.document.to_snapshot())
        .expect("持久原卡");
        storage::WorkspaceAgentRoleStore::new(paths.app_db_path().to_owned())
            .create(&card, name, true)
            .expect("角色登记");
        let prepared =
            PreparedProject::load_for_desktop(paths.clone(), area.clone()).expect("准备 session");
        fixtures.push((area, runtime, prepared));
    }
    let mut test = TestAppContext::single();
    let (host, cx) = test
        .add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store));
    cx.run_until_parked();
    let mut sessions = vec![];
    for (area, runtime, prepared) in fixtures {
        let session = cx.new(|cx| {
            let mut app = MultisoulApp::from_background_project(prepared, cx);
            app.runtime = runtime;
            app.desktop_host = Some(host.downgrade());
            app.workspace_route_id = Some(format!("route-{}", area.id));
            app
        });
        sessions.push((area, session));
    }
    host.update_in(cx, |app, window, cx| {
        crate::workspace::desktop::sessions::register(
            paths.app_db_path().to_owned(),
            window.window_handle(),
            host.clone(),
            cx,
        );
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.expanded_area_id = Some(sessions[0].0.id.clone());
        for (area, session) in &sessions {
            app.desktop_workspace
                .sessions
                .insert((workspace.id.clone(), area.id.clone()), session.clone());
        }
    });
    run(host, cx, paths, workspace, other, sessions, writes);
}
