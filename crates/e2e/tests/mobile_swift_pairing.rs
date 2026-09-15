use app::{MobileAccess, MobileServer};
use norma::mobile::ServerIdentity;
use std::{
    fs,
    process::Command,
    time::{Duration, Instant},
};
use storage::{AppStore, MobilePermission, MobileProjectAccess};

/// 原生 Swift URLSession/Keychain 经真实 Rust WSS 完成配对、权限回查、撤销，错误证书不得产生申请。
///
/// 数据构造：隔离 SQLite、随机证书、两个正常回环端口、一个只接受TCP却不回应TLS的端口；两张真实空卡。
/// 执行过程：错误 pin → 首地址握手停滞时扫码/回查 → 只读创建拒绝 → 重扫 → 显式升权后创建/删除 → 撤销。
/// 预期结果：正常地址2s内可用（小于坏地址5s超时），错误pin无申请；创建不启动Agent、重放不重复、撤销拒绝。
#[test]
#[ignore = "requires swift build --package-path ios and NORMA_SWIFT_PROBE"]
fn swift_native_client_pairs_authenticates_and_rejects_wrong_pin() {
    let probe = std::env::var("NORMA_SWIFT_PROBE").expect("显式 Swift 验证程序");
    let dir = tempfile::tempdir().expect("隔离目录");
    let store = AppStore::new(dir.path().join("app.sqlite"));
    let project = store
        .workspace_identity_store()
        .resolve_or_create(dir.path(), "Swift")
        .expect("项目");
    let access =
        MobileAccess::with_identity_for_e2e(store, ServerIdentity::generate().expect("身份"));
    let (source, requests) = async_channel::bounded::<app::LiveSnapshotRequest>(8);
    let receive = requests.clone();
    let (command_source, command_requests) = async_channel::bounded::<app::LiveCommandRequest>(8);
    let receive_commands = command_requests.clone();
    let canvas_store = storage::CanvasStore::new(dir.path().join("native-project.sqlite"));
    let snapshots = std::thread::spawn(move || {
        // Runtime stays on its owning thread, just as in the actual GPUI host.
        let mut runtime = runtime::Runtime::new();
        let rect = canvas::CanvasRect::new(0.0, 0.0, 640.0, 400.0);
        runtime
            .create_mobile_agent_card(canvas::AgentProfile::Codex, None, None, rect)
            .expect("空 Codex 卡");
        runtime
            .create_mobile_agent_card(canvas::AgentProfile::Claude, None, None, rect)
            .expect("空 Claude 卡");
        loop {
            if let Ok(mut request) = receive.try_recv() {
                for project in &mut request.projects {
                    project.state = "open".into();
                    project.route_id = Some("swift-fixture-route".into());
                    project.card_count = Some(runtime.mobile_card_snapshots().len());
                    project.cards = request
                        .include_cards
                        .then(|| runtime.mobile_card_snapshots());
                }
                let _ = request.reply.try_send(request.projects);
            }
            if let Ok(call) = receive_commands.try_recv() {
                assert!(call.lease.begin(), "fixture 仅执行正式 gateway 授权的请求");
                let node = match &call.request.command {
                    app::MobileCommand::RealtimeTool { .. } => {
                        panic!("手机网络不能构造 Mac 内部实时工具命令")
                    }
                    app::MobileCommand::TerminalInput { .. }
                    | app::MobileCommand::AcceptDelivery { .. }
                    | app::MobileCommand::FeedbackDelivery { .. }
                    | app::MobileCommand::DecidePlanTodo { .. }
                    | app::MobileCommand::GeneratePlan { .. }
                    | app::MobileCommand::DecideApproval { .. }
                    | app::MobileCommand::QueueMessage { .. }
                    | app::MobileCommand::SendMessage { .. }
                    | app::MobileCommand::InterruptTurn { .. } => {
                        panic!("空卡fixture不能执行Provider消息；使用真实messages测试")
                    }
                    app::MobileCommand::CreateCard {
                        profile,
                        title,
                        definition_id: _,
                    } => {
                        let profile = match profile {
                            app::MobileProvider::Codex => canvas::AgentProfile::Codex,
                            app::MobileProvider::Claude => canvas::AgentProfile::Claude,
                            app::MobileProvider::Cursor => canvas::AgentProfile::Cursor,
                        };
                        runtime
                            .create_mobile_agent_card(profile, None, title.clone(), rect)
                            .expect("真实 Runtime 空卡")
                    }
                    app::MobileCommand::DeleteCard {
                        expected,
                        confirm_busy,
                    } => {
                        runtime
                            .validate_mobile_card_deletion(expected, *confirm_busy)
                            .expect("准确删除身份");
                        assert!(
                            runtime.remove_node(expected.node_id),
                            "fixture 删除准确目标"
                        );
                        expected.node_id
                    }
                };
                runtime
                    .save_canvas_snapshot(&canvas_store)
                    .expect("成功前实际保存 SQLite");
                let _ = call.reply.send(Ok(serde_json::json!({"node_id":node})));
            }
            if receive.is_closed() && receive_commands.is_closed() {
                break;
            }
            // Test-only 2ms loop serves both bounded channels; production uses GPUI async receivers.
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(runtime.live_len(), 0, "原生客户端创建空卡不能启动 Agent");
    });
    let server = MobileServer::start_with_sources(
        access,
        vec![
            "127.0.0.1:0".parse().expect("回环 A"),
            "127.0.0.1:0".parse().expect("回环 B"),
        ],
        Some(source),
        Some(command_source),
    )
    .expect("服务");
    let qr = dir.path().join("offer.json");
    let stalled = std::net::TcpListener::bind("127.0.0.1:0").expect("隔离停滞地址");
    let stalled_endpoint = format!("wss://{}/mobile", stalled.local_addr().expect("停滞端口"));
    stalled.set_nonblocking(true).expect("fixture不能阻塞回收");
    let lifetime = std::sync::Arc::new(());
    let running = std::sync::Arc::downgrade(&lifetime);
    let stalled_worker = std::thread::spawn(move || {
        let mut connections = Vec::new();
        while running.upgrade().is_some() {
            match stalled.accept() {
                Ok((connection, _)) => connections.push(connection),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => panic!("停滞fixture接收失败：{error}"),
            }
            //20ms只用于fixture接受/回收；不发送TLS响应，故原客户端会等满5s。
            std::thread::sleep(Duration::from_millis(20));
        }
    });
    fs::write(&qr, b"").expect("创建待填充 QR 文件");
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&qr, fs::Permissions::from_mode(0o600)).expect("仅当前用户可读");
    for mode in ["wrong-pin", "normal"] {
        // Each run may start Xcode; issue a fresh offer so earlier verification does not spend its TTL.
        let mut offer: serde_json::Value = serde_json::from_str(
            &server
                .begin_pairing("Swift Mac")
                .expect("刷新签发")
                .to_json()
                .expect("QR"),
        )
        .expect("QR对象");
        if mode == "normal" {
            if std::env::var_os("NORMA_WATCH_LIFECYCLE").is_some() {
                fs::write(dir.path().join("watch-lifecycle"), "").expect("独立满额订阅验收");
            }
            offer["endpoints"]
                .as_array_mut()
                .expect("地址列表")
                .insert(0, serde_json::json!(stalled_endpoint));
            fs::write(dir.path().join("endpoint-latency"), "").expect("原生延迟验收标记");
        }
        fs::write(&qr, offer.to_string()).expect("刷新QR");
        let log = dir.path().join(format!("{mode}.log"));
        let mut child = Command::new(&probe)
            .arg(&qr)
            .arg(mode)
            .stdout(fs::File::create(&log).expect("日志"))
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("Swift 子进程");
        // 120s = probe 的 30s 授权期限 + 90s Xcode 启动/模拟器调度余量，不允许无限挂起。
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut approved = None;
        let mut revoked = false;
        let mut control_granted = false;
        let result = loop {
            if let Some(status) = child.try_wait().expect("进程状态") {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().expect("停止超时验证");
                child.wait().expect("回收子进程");
                panic!("Swift 验证超时，模式 {mode}");
            }
            if let Some(pending) = server.pending().expect("申请") {
                if mode == "wrong-pin" {
                    child.kill().expect("停止错误证书验证");
                    child.wait().expect("回收子进程");
                    panic!("错误证书不得产生申请");
                }
                assert!(approved.is_none(), "已配对手机重扫不得提交第二次申请");
                server
                    .approve(
                        &pending.request_id,
                        &[MobileProjectAccess {
                            workspace_id: project.workspace_id.clone(),
                            access: MobilePermission::Read,
                        }],
                    )
                    .expect("仅隔离测试的 Mac 批准");
                approved = Some(pending.device_id);
            }
            if !control_granted && dir.path().join("enable-control").exists() {
                server
                    .replace_permissions(
                        approved.as_deref().expect("先配对"),
                        1,
                        &[MobileProjectAccess {
                            workspace_id: project.workspace_id.clone(),
                            access: MobilePermission::Control,
                        }],
                    )
                    .expect("仅测试显式升级控制权限");
                control_granted = true;
            }
            if !revoked && dir.path().join("authorized").exists() {
                server
                    .revoke(approved.as_deref().expect("必须先批准"))
                    .expect("撤销");
                revoked = true;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let output = fs::read_to_string(log).expect("验证输出");
        assert!(
            result.success(),
            "Swift 验证失败，模式 {mode}，输出 {output}"
        );
        if mode == "normal" && std::env::var_os("NORMA_WATCH_LIFECYCLE").is_some() {
            assert!(
                output.contains("NATIVE_WATCH_LIFECYCLE_OK"),
                "必须实际跑完24次原生连接替换，不能只验配对"
            );
            for line in output
                .lines()
                .filter(|line| line.contains("watch_replacement"))
            {
                println!("{line}");
            }
        }
        if mode == "wrong-pin" {
            assert!(
                output.contains("WRONG_PIN_REJECTED"),
                "必须拒绝证书，而非其他连接错误"
            );
            assert!(
                server.pending().expect("申请状态").is_none(),
                "错误 pin 不得提交任何配对数据"
            );
        } else {
            assert!(
                output.contains("NATIVE_ENDPOINT_LATENCY_OK"),
                "必须通过坏首地址的真实原生延迟验收"
            );
            assert!(
                output.contains("NATIVE_DELETE_RECEIPT_OK"),
                "原生客户端必须删除准确卡片并重连回查，保留另一张卡"
            );
            assert!(
                output.contains("NATIVE_CREATE_RECEIPT_OK"),
                "原生客户端必须保存指令并经 WSS 创建卡片 / 回查 / 拒绝重复参数"
            );
            assert!(
                output.contains("SCOPED_PROJECT_CARD_SNAPSHOT_OK"),
                "Swift 必须实际读取项目和卡片并拒绝未授权范围"
            );
            assert!(
                output.contains("RESCAN_AUTHORIZED_WITHOUT_NEW_PAIRING"),
                "另一地址重扫必须认证成功，而非创建第二个配对"
            );
            assert_eq!(
                server.devices().expect("设备").len(),
                1,
                "重扫不能增加手机记录"
            );
            assert!(
                server.pending().expect("待审批").is_none(),
                "重扫不能留下新的待批准申请"
            );
            assert!(
                output.contains("PAIR_AUTH_REVOKE_OK"),
                "必须完整认证并观察撤销"
            );
            assert!(revoked, "Mac 必须实际撤销该手机");
        }
    }
    requests.close();
    command_requests.close();
    snapshots.join().expect("回收快照 fixture");
    drop(lifetime);
    stalled_worker.join().expect("回收停滞地址");
}
