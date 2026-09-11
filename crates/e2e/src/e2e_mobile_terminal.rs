use crate::mobile_wire as wire;
use crate::{e2e_mobile_snapshot::fetch_action, harness::make_app};
use app::{MobileAccess, MobileServer, MobileSources};
use gpui::TestAppContext;
use serde_json::{json, Value};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use storage::{AppStore, MobilePermission, MobileProjectAccess};

/// 终端通过正式TLS/WSS读取和输入，权限/会话/屏幕/幂等记录共同限制原PTY副作用。
///
/// 数据构造：1张80×24录制Codex卡；只读revision1→控制revision2；9种快捷键与1段文本。
/// 执行过程：只读查看/拒绝输入→升权→写文本/回查/重放→旧屏幕拒绝→快捷键→订阅→输出更新→撤权。
/// 预期结果：文字无附带Enter；每个快捷键准确一次；正文按增量推送；旧scope、只读、撤权不泄漏或写入。
#[test]
fn e2e_mobile_terminal_read_input_events_and_authorization() {
    let mut test = TestAppContext::single();
    let sockets = tempfile::tempdir().expect("隔离路由");
    test.update(|cx| {
        app::install_norma_endpoint_for_e2e(sockets.path().join("norma.sock"), cx).expect("路由")
    });
    let writes = Arc::new(Mutex::new(Vec::new()));
    let output = Arc::new(Mutex::new(None));
    let recording = writes.clone();
    let sender = output.clone();
    let factory: runtime::PtyFactory = Box::new(move |_, _, _, tx| {
        *sender.lock().expect("输出") = Some(tx.clone());
        Ok(Box::new(terminal::TestPtyBackend::recording(
            tx,
            recording.clone(),
        )))
    });
    let (app, cx) = make_app(&mut test, factory);
    let workspace = tempfile::tempdir().expect("项目");
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.enter_canvas_and_register_norma_route_for_e2e(workspace.path(), window, cx)
        })
    });
    let node = app.update(cx, |app, _| {
        app.runtime
            .spawn_agent(
                canvas::AgentProfile::Codex,
                canvas::CanvasRect::new(0., 0., 640., 400.),
                80,
                24,
            )
            .expect("原卡")
    });
    let store = AppStore::new(app.read_with(cx, |app, _| app.app_database_path_for_e2e()));
    let project = store
        .workspace_identity_store()
        .resolve_or_create(workspace.path(), "Terminal")
        .expect("项目身份")
        .workspace_id;
    store
        .mobile_access_store()
        .register(
            "phone-a",
            "iPhone",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: project.clone(),
                access: MobilePermission::Read,
            }],
        )
        .expect("只读");
    let identity = norma::mobile::ServerIdentity::generate().expect("证书");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let server = MobileServer::start_with_mobile_sources(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        MobileSources {
            snapshots: app.read_with(cx, |app, cx| app.mobile_snapshot_source_for_e2e(cx)),
            commands: app.read_with(cx, |app, cx| app.mobile_command_source_for_e2e(cx)),
            terminals: app.read_with(cx, |app, cx| app.mobile_terminal_source_for_e2e(cx)),
            ..Default::default()
        },
    )
    .expect("WSS");
    let snapshot = fetch_action(
        cx,
        &server,
        &cert,
        &machine,
        json!({"type":"snapshot","device_id":"phone-a","token":"a".repeat(64),"workspace_id":project}),
    );
    let route = snapshot["result"]["projects"][0]["route_id"]
        .as_str()
        .expect("route");
    let card = snapshot["result"]["projects"][0]["cards"][0].clone();
    let scope = json!({"workspace_id":project,"route_id":route,"node_id":node,"run_id":card["run_id"],"session_id":null});
    let read =
        json!({"type":"terminal","device_id":"phone-a","token":"a".repeat(64),"request":scope});
    let first = fetch_action(cx, &server, &cert, &machine, read.clone());
    assert_eq!(
        first["result"]["terminal"]["page"]["rows"]
            .as_array()
            .map(Vec::len),
        Some(24),
        "只读可查看真实原终端"
    );
    let mut action = json!({"type":"command","device_id":"phone-a","token":"a".repeat(64),"request":{
        "operation_id":"text-a","workspace_id":project,"route_id":route,"grant_revision":1,
        "command":{"type":"terminal_input","expected":card,"screen_signature":first["result"]["terminal"]["page"]["screen_signature"],"input_sequence":0,"input":{"type":"text","text":"hello"}}}});
    assert_eq!(
        fetch_action(cx, &server, &cert, &machine, action.clone())["error"],
        "unauthorized",
        "只读不可输入"
    );
    server
        .replace_permissions(
            "phone-a",
            1,
            &[MobileProjectAccess {
                workspace_id: project.clone(),
                access: MobilePermission::Control,
            }],
        )
        .expect("升权");
    assert_eq!(
        fetch_action(cx, &server, &cert, &machine, action.clone())["error"],
        "grant_changed",
        "旧授权不沿用"
    );
    action["request"]["grant_revision"] = json!(2);
    let _ = fetch_action(cx, &server, &cert, &machine, action.clone());
    let query = json!({"type":"receipt","device_id":"phone-a","token":"a".repeat(64),"workspace_id":project,"operation_id":"text-a"});
    let deadline = Instant::now() + Duration::from_secs(5); //前台+SQLite+TLS总预算5s。
    let receipt = loop {
        let response = fetch_action(cx, &server, &cert, &machine, query.clone());
        if response["result"]["receipt"]["state"] != "accepted" {
            break response;
        }
        assert!(Instant::now() < deadline, "输入必须有界持久化");
    };
    assert_eq!(
        receipt["result"]["receipt"]["result"]["status"], "written",
        "只确认写入PTY"
    );
    assert!(
        receipt["result"]["receipt"]["result"]["turn_id"].is_null(),
        "不得伪造Agent已执行"
    );
    assert_eq!(
        fetch_action(cx, &server, &cert, &machine, action.clone())["result"]["receipt"]["state"],
        "succeeded",
        "重放只回查"
    );
    assert_eq!(
        *writes.lock().expect("写入"),
        vec![b"hello".to_vec()],
        "文本没有Enter且只发送一次"
    );
    action["request"]["operation_id"] = json!("stale-screen");
    action["request"]["command"]["input"] = json!({"type":"enter"});
    let _ = fetch_action(cx, &server, &cert, &machine, action.clone());
    // The next read drains queued dispatch before inspecting the original recorder.
    let _ = fetch_action(cx, &server, &cert, &machine, read.clone());
    assert_eq!(writes.lock().expect("写入").len(), 1, "旧屏幕Enter不得输入");
    let keys = [
        "enter",
        "tab",
        "escape",
        "up",
        "down",
        "left",
        "right",
        "backspace",
        "ctrl_c",
    ];
    for key in keys {
        let page = fetch_action(cx, &server, &cert, &machine, read.clone())["result"]["terminal"]
            ["page"]
            .clone();
        action["request"]["operation_id"] = json!(format!("key-{key}"));
        action["request"]["command"]["screen_signature"] = page["screen_signature"].clone();
        action["request"]["command"]["input_sequence"] = page["input_sequence"].clone();
        action["request"]["command"]["input"] = json!({"type":key});
        let _ = fetch_action(cx, &server, &cert, &machine, action.clone());
    }
    let _ = fetch_action(cx, &server, &cert, &machine, read.clone());
    assert_eq!(
        *writes.lock().expect("写入"),
        vec![
            b"hello".to_vec(),
            b"\r".to_vec(),
            b"\t".to_vec(),
            b"\x1b".to_vec(),
            b"\x1b[A".to_vec(),
            b"\x1b[B".to_vec(),
            b"\x1b[D".to_vec(),
            b"\x1b[C".to_vec(),
            b"\x7f".to_vec(),
            b"\x03".to_vec()
        ],
        "全部快捷键沿用原编码且无多余输入"
    );
    let mut wrong = read.clone();
    wrong["request"]["run_id"] = json!("old-run");
    assert_eq!(
        fetch_action(cx, &server, &cert, &machine, wrong)["error"],
        "session_changed",
        "旧run不可读"
    );
    let mut wrong = read.clone();
    wrong["request"]["workspace_id"] = json!("ungranted");
    assert_eq!(
        fetch_action(cx, &server, &cert, &machine, wrong)["error"],
        "unauthorized",
        "未授权项目不可读"
    );
    let (tx, rx) = std::sync::mpsc::channel();
    let address = server.addresses()[0];
    let peer_cert = cert.clone();
    let peer_machine = machine.clone();
    let mut watch = read;
    watch["watch"] = json!(true);
    let peer = std::thread::spawn(move || {
        let mut socket = wire::connect(address, peer_cert);
        tx.send(wire::exchange(&mut socket, &peer_machine, "watch-a", watch))
            .expect("首屏");
        while let Ok(message) = socket.read() {
            if let Ok(text) = message.to_text() {
                let value: Value = serde_json::from_str(text).expect("事件");
                if tx.send(value).is_err() {
                    break;
                }
            }
        }
    });
    let wait = |cx: &mut gpui::VisualTestContext| {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            cx.executor().advance_clock(Duration::from_millis(20));
            cx.run_until_parked();
            if let Ok(value) = rx.try_recv() {
                break value;
            }
            assert!(Instant::now() < deadline, "终端事件必须到达");
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    let baseline = wait(cx);
    assert_eq!(
        baseline["result"]["terminal_event"]["sequence"], 0,
        "必须先有首屏"
    );
    let changed_at = Instant::now();
    output
        .lock()
        .expect("输出")
        .as_ref()
        .expect("同PTY")
        .send(terminal::PtyEvent::Output(b"changed-line".to_vec()))
        .expect("输出");
    let changed = loop {
        let next = wait(cx);
        if !next["result"]["terminal_event"]["page"].is_null() {
            break next;
        }
    };
    assert!(
        changed_at.elapsed() < Duration::from_millis(500),
        "输出更新不得等待2s心跳；50ms合并+450ms测试调度预算"
    );
    assert_eq!(
        changed["result"]["terminal_event"]["page"]["rows"],
        json!(["changed-line"]),
        "仅传改变的行"
    );
    assert_eq!(
        changed["result"]["terminal_event"]["remove_count"], 1,
        "不能反复传整屏"
    );
    server.revoke("phone-a").expect("撤权断开");
    peer.join().expect("订阅随撤权结束");
    assert_eq!(
        app.read_with(cx, |app, _| app.runtime.live_len()),
        1,
        "全链路只有原PTY"
    );
}

/// 真实Codex原卡用原生Swift手机完成终端输入和实际回复。
/// 数据构造：一个80×24原生Codex、一条禁止工具的固定回复提示；正式TLS/WSS、隔离App数据库。
/// 执行过程：原CLI启动就绪→Swift配对→粘贴/Enter→原会话正文与增量→关闭测试PTY。
/// 预期结果：准确固定回复、唯一PTY和run；旧run拒绝；仅关闭隔离测试自己的进程。
#[test]
#[ignore = "requires real Codex and NORMA_SWIFT_PROBE"]
fn original_codex_terminal_via_swift() {
    native_terminal(canvas::AgentProfile::Codex);
}

/// 真实Claude原卡通过相同Swift终端输入，不使用第二执行器。
/// 数据构造：一个80×24原生Claude、一条固定无工具提示，复用已信任仓库cwd。
/// 执行过程：原生启动→手机填入正文→Enter→真实历史/增量→隔离PTY释放。
/// 预期结果：原卡/会话保留，只有一条user；写入回执不冒充真实助手回复。
#[test]
#[ignore = "requires real Claude and NORMA_SWIFT_PROBE"]
fn original_claude_terminal_via_swift() {
    native_terminal(canvas::AgentProfile::Claude);
}

fn native_terminal(profile: canvas::AgentProfile) {
    let probe = std::env::var("NORMA_SWIFT_PROBE").expect("原生客户端");
    let mut test = TestAppContext::single();
    let directory = tempfile::tempdir().expect("隔离路由");
    test.update(|cx| {
        app::install_norma_endpoint_for_e2e(directory.path().join("norma.sock"), cx).expect("路由")
    });
    let (app, cx) = crate::harness::make_app_real(&mut test);
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates")
        .parent()
        .expect("repo");
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.enter_canvas_and_register_norma_route_for_e2e(workspace, window, cx)
        })
    });
    let node = app.update(cx, |app, _| {
        app.runtime
            .spawn_mobile_agent_card(
                profile,
                None,
                canvas::CanvasRect::new(0., 0., 640., 400.),
                80,
                24,
            )
            .expect("原生空卡")
    });
    crate::harness::drain_until(
        &app,
        cx,
        |runtime| (runtime.mobile_card_snapshots()[0].state == "ready").then_some(()),
        Duration::from_secs(30),
    )
    .expect("原生CLI30s内就绪");
    let original = app.read_with(cx, |app, _| {
        app.runtime
            .mobile_agent_run_id(node)
            .expect("run")
            .to_owned()
    });
    crate::e2e_mobile_conversation::verify_native(&app, cx, workspace, &probe, "terminal");
    app.update(cx, |app, _| {
        assert_eq!(
            app.runtime.mobile_agent_run_id(node),
            Some(original.as_str()),
            "手机输入不能重开run"
        );
        assert_eq!(app.runtime.live_len(), 1, "唯一原PTY");
        app.runtime.kill_all();
        assert!(
            app.runtime.mobile_agent_run_id(node).is_none(),
            "只清理自己的测试进程"
        );
    });
}
