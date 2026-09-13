use crate::e2e_mobile_snapshot::fetch_action;
use crate::harness::{echo_factory, make_app, project_store};
use app::{MobileAccess, MobileServer};
use canvas::{AgentProfile, CanvasRect};
use gpui::TestAppContext;
use serde_json::json;
use storage::{AppStore, MobilePermission, MobileProjectAccess};

/// 手机按需读取未开窗的项目，重复访问复用同一运行身份且不启动 Agent。
///
/// 数据构造：
///   1 个首页窗口；授权项目含 2 张空 Agent 卡，另 1 个项目未授权；
///   卡片尺寸沿用默认 640×400，只有存储快照，无 PTY、无真实 Provider。
/// 执行过程：
///   1. 在隔离数据库保存项目卡片并启动真实回环 WSS；
///   2. 手机先读目录，再连续读取指定项目两次；
///   3. 比较卡片身份、项目路由和实际窗口数量，再尝试越权读取。
/// 预期结果：
///   - 正断言：两张卡可见、两次 route_id 相同；
///   - 负断言：窗口不增加、卡片没有 run_id，未授权项目读取失败。
#[test]
fn e2e_mobile_loads_project_without_opening_window_or_starting_agents() {
    let mut cx = TestAppContext::single();
    let route_dir = tempfile::tempdir().expect("隔离路由目录");
    cx.update(|cx| {
        app::install_norma_endpoint_for_e2e(route_dir.path().join("norma.sock"), cx)
            .expect("安装生产项目路由")
    });
    let (app, cx) = make_app(&mut cx, echo_factory());
    let root = tempfile::tempdir().expect("后台项目");
    let private = tempfile::tempdir().expect("未授权项目");
    let store = AppStore::new(app.read_with(cx, |app, _| app.app_database_path_for_e2e()));
    let identity_store = store.workspace_identity_store();
    let project = identity_store
        .resolve_or_create(root.path(), "Background")
        .expect("项目身份");
    let private = identity_store
        .resolve_or_create(private.path(), "Private")
        .expect("私有身份");
    let mut runtime = runtime::Runtime::new();
    let rect = CanvasRect::new(0.0, 0.0, 640.0, 400.0);
    let codex = runtime
        .create_mobile_agent_card(AgentProfile::Codex, None, None, rect)
        .expect("Codex 卡");
    let claude = runtime
        .create_mobile_agent_card(AgentProfile::Claude, None, None, rect)
        .expect("Claude 卡");
    runtime
        .save_canvas_snapshot(&project_store(&app, cx, root.path()).canvas_store())
        .expect("保存原卡");
    store
        .mobile_access_store()
        .register(
            "phone-a",
            "iPhone",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: project.workspace_id.clone(),
                access: MobilePermission::Read,
            }],
        )
        .expect("授权项目读取");
    let identity = norma::mobile::ServerIdentity::generate().expect("TLS 身份");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let source = app
        .read_with(cx, |app, cx| app.mobile_snapshot_source_for_e2e(cx))
        .expect("生产读取入口");
    let server = MobileServer::start_with_snapshots(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环地址")],
        Some(source),
    )
    .expect("WSS 服务");
    let windows = cx.update(|_, cx| cx.windows().len());
    let action = json!({"type":"snapshot", "device_id":"phone-a", "token":"a".repeat(64), "workspace_id":project.workspace_id});
    let first = fetch_action(cx, &server, &cert, &machine, action.clone());
    let first = &first["result"]["projects"][0];
    assert_eq!(
        first["state"], "open",
        "未开窗的已授权项目也应可以加载: {first}"
    );
    assert_eq!(first["cards"][0]["node_id"], codex, "保留原 Codex 卡身份");
    assert_eq!(first["cards"][1]["node_id"], claude, "保留原 Claude 卡身份");
    assert!(first["cards"][0]["run_id"].is_null(), "读项目不启动 Codex");
    assert!(first["cards"][1]["run_id"].is_null(), "读项目不启动 Claude");
    let request = norma::protocol::WireRequest {
        protocol_version: norma::protocol::CURRENT_PROTOCOL_VERSION,
        client_version: None,
        request_id: 1,
        operation_id: None,
        authorization: None,
        source_workspace_id: Some(project.workspace_id.clone()),
        source_workspace_route_id: first["route_id"].as_str().map(str::to_owned),
        target_workspace_id: None,
        source_node_id: None,
        command: norma::protocol::CardCommand::List,
    };
    let listed = cx
        .cx
        .update(|cx| app::submit_norma_request_for_e2e(request.clone(), cx));
    cx.run_until_parked();
    assert!(
        matches!(listed.try_recv().expect("后台CLI结果").result, Ok(norma::protocol::CardCommandResult::Listed { cards }) if cards.len() == 2),
        "后台项目的CLI读取也必须可用"
    );
    let focus = cx.cx.update(|cx| {
        app::submit_norma_request_for_e2e(
            norma::protocol::WireRequest {
                command: norma::protocol::CardCommand::Focus { node_id: codex },
                ..request
            },
            cx,
        )
    });
    cx.run_until_parked();
    assert!(
        matches!(
            focus.try_recv().expect("纯UI操作结果").result,
            Err(norma::protocol::CardCommandError::Unsupported { .. })
        ),
        "无窗口的聚焦必须明确拒绝，不能隐式开窗"
    );
    let second = fetch_action(cx, &server, &cert, &machine, action);
    assert_eq!(
        second["result"]["projects"][0]["route_id"], first["route_id"],
        "重复访问不能重建项目实例"
    );
    assert_eq!(
        cx.update(|_, cx| cx.windows().len()),
        windows,
        "手机读取不能创建或隐藏桌面窗口"
    );
    let denied = fetch_action(
        cx,
        &server,
        &cert,
        &machine,
        json!({"type":"snapshot", "device_id":"phone-a", "token":"a".repeat(64), "workspace_id":private.workspace_id}),
    );
    assert_eq!(denied["error"], "unauthorized", "未授权项目不能被后台加载");
}

/// 关闭手机服务未开启的项目必须释放原进程和路由，即使仍有测试/UI持有实体。
///
/// 数据构造：1 个项目窗口和 1 个 echo PTY，服务未开启；外部 Entity 引用故意保留。
/// 执行过程：打开项目 → 创建原卡 → 移除窗口 → 执行生产关窗处理 → 查询路由和终端。
/// 预期结果：不要求后台保活；原路由不能留在注册表；终端不再 Running。
#[test]
fn e2e_project_close_without_mobile_stops_even_with_entity_reference() {
    let mut owner = TestAppContext::single();
    let socket = tempfile::tempdir().expect("隔离路由");
    owner.update(|cx| {
        app::install_norma_endpoint_for_e2e(socket.path().join("norma.sock"), cx).expect("路由")
    });
    let killed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let recorded = killed.clone();
    let (app, cx) = make_app(
        &mut owner,
        Box::new(move |_, _, _, tx| {
            Ok(Box::new(ShutdownPty {
                inner: terminal::TestPtyBackend::echo(tx),
                killed: recorded.clone(),
            }))
        }),
    );
    let root = tempfile::tempdir().expect("项目");
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.enter_canvas_and_register_norma_route_for_e2e(
                &root.path().canonicalize().expect("规范项目路径"),
                window,
                cx,
            )
        })
    });
    app.update(cx, |app, _| {
        app.runtime
            .spawn_agent(
                AgentProfile::Codex,
                CanvasRect::new(0., 0., 640., 400.),
                80,
                24,
            )
            .expect("echo 原卡")
    });
    let store = AppStore::new(app.read_with(cx, |app, _| app.app_database_path_for_e2e()));
    let id = store
        .workspace_identity_store()
        .resolve_or_create(root.path(), "Project")
        .expect("身份")
        .workspace_id;
    cx.update(|window, _| window.remove_window());
    let retain = cx
        .cx
        .update(app::MultisoulApp::project_windows_closed_for_e2e);
    assert!(!retain, "未启用手机服务不能保活");
    assert!(
        app.read_with(cx, |app, cx| app.loaded_project_for_e2e(&id, cx))
            .is_none(),
        "关闭项目须从实例注册表移除"
    );
    assert!(
        killed.load(std::sync::atomic::Ordering::Acquire),
        "外部 Entity 引用不能使关窗后的 PTY 继续运行"
    );
    assert!(
        app.read_with(cx, |app, _| app.norma_workspace_identity_for_e2e())
            .is_none(),
        "停止后不能通过旧路由执行"
    );
}

/// 手机服务开启后关窗保留原卡；桌面重开接回同一实体、路由和 PTY。
///
/// 数据构造：1 个项目、1 个 echo Codex 原卡、已开启的回环 WSS；不访问真实 Provider。
/// 执行过程：记录卡片运行身份 → 关窗 → 检查无窗口仍保活 → 生产开窗入口重开 → 比较身份。
/// 预期结果：后台无新增窗口；重开只出现1个窗口且原卡run不变；主动停止后不再接受远程路由。
#[test]
fn e2e_mobile_background_reopens_same_project_and_terminal() {
    let mut owner = TestAppContext::single();
    let socket = tempfile::tempdir().expect("隔离路由");
    owner.update(|cx| {
        app::install_norma_endpoint_for_e2e(socket.path().join("norma.sock"), cx).expect("路由")
    });
    let (app, cx) = make_app(&mut owner, echo_factory());
    let root = tempfile::tempdir().expect("项目");
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.enter_canvas_and_register_norma_route_for_e2e(
                &root.path().canonicalize().expect("规范项目路径"),
                window,
                cx,
            )
        })
    });
    let node = app.update(cx, |app, _| {
        app.runtime
            .spawn_agent(
                AgentProfile::Codex,
                CanvasRect::new(0., 0., 640., 400.),
                80,
                24,
            )
            .expect("echo 原卡")
    });
    let before = app.read_with(cx, |app, _| app.runtime.mobile_card_snapshots().remove(0));
    let store = AppStore::new(app.read_with(cx, |app, _| app.app_database_path_for_e2e()));
    let project = store
        .workspace_identity_store()
        .resolve_or_create(root.path(), "Project")
        .expect("身份");
    store
        .mobile_access_store()
        .register(
            "phone-a",
            "iPhone",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: project.workspace_id.clone(),
                access: MobilePermission::Read,
            }],
        )
        .expect("授权");
    let identity = norma::mobile::ServerIdentity::generate().expect("身份");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let source = app.read_with(cx, |app, cx| app.mobile_snapshot_source_for_e2e(cx));
    let server = std::sync::Arc::new(
        MobileServer::start_with_snapshots(
            MobileAccess::with_identity_for_e2e(store, identity),
            vec!["127.0.0.1:0".parse().expect("回环")],
            source,
        )
        .expect("手机服务"),
    );
    app.update(cx, |app, cx| {
        app.retain_mobile_server_for_e2e(server.clone(), cx)
    });
    let action = json!({"type":"snapshot","device_id":"phone-a","token":"a".repeat(64),"workspace_id":project.workspace_id});
    let first = fetch_action(cx, &server, &cert, &machine, action.clone());
    cx.update(|window, _| window.remove_window());
    assert!(
        cx.cx
            .update(app::MultisoulApp::project_windows_closed_for_e2e),
        "手机服务开启时关窗应保活"
    );
    assert_eq!(
        cx.cx.update(|cx| cx.windows().len()),
        0,
        "后台不能用隐藏窗口替代"
    );
    let background = fetch_action(cx, &server, &cert, &machine, action.clone());
    assert_eq!(
        background["result"]["projects"][0]["route_id"], first["result"]["projects"][0]["route_id"],
        "关窗不旋转路由"
    );
    assert_eq!(
        background["result"]["projects"][0]["cards"][0]["run_id"],
        json!(before.run_id),
        "关窗不替换原卡进程"
    );
    let (reopened, window) = cx
        .cx
        .update(|cx| {
            app::MultisoulApp::reopen_project_window_for_e2e(
                &app,
                root.path().canonicalize().expect("规范路径"),
                cx,
            )
        })
        .expect("生产开窗");
    assert_eq!(
        reopened.entity_id(),
        app.entity_id(),
        "桌面必须复用后台同一实体"
    );
    assert_eq!(
        cx.cx.update(|cx| cx.windows().len()),
        1,
        "只创建用户要求的窗口"
    );
    assert_eq!(
        reopened.read_with(cx, |app, _| app.runtime.mobile_card_snapshots()[0]
            .run_id
            .clone()),
        before.run_id,
        "附着窗口不能重启会话"
    );
    assert_eq!(
        reopened.read_with(cx, |app, _| app.runtime.mobile_card_snapshots()[0].node_id),
        node,
        "原卡身份不变"
    );
    window
        .update(cx, |_, window, _| window.remove_window())
        .expect("再次关窗");
    assert!(
        cx.cx
            .update(app::MultisoulApp::project_windows_closed_for_e2e),
        "再次关窗仍可后台运行"
    );
    cx.cx
        .update(app::MultisoulApp::stop_background_projects_for_e2e);
    assert!(
        app.read_with(cx, |app, _| app.norma_workspace_identity_for_e2e())
            .is_none(),
        "主动退出须注销原路由"
    );
    assert!(
        app.read_with(cx, |app, cx| app
            .loaded_project_for_e2e(&project.workspace_id, cx))
            .is_none(),
        "主动退出不保留后台宿主"
    );
    assert!(
        !cx.cx
            .update(app::MultisoulApp::project_windows_closed_for_e2e),
        "主动退出后没有服务继续保活"
    );
}

struct ShutdownPty {
    inner: terminal::TestPtyBackend,
    killed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl terminal::PtyBackend for ShutdownPty {
    fn write(&mut self, bytes: &[u8]) -> Result<(), terminal::PtyError> {
        self.inner.write(bytes)
    }
    fn resize(&mut self, cols: u16, rows: u16) -> Result<(), terminal::PtyError> {
        self.inner.resize(cols, rows)
    }
    fn kill(&mut self) {
        self.killed
            .store(true, std::sync::atomic::Ordering::Release);
        self.inner.kill();
    }
}
