use crate::harness::{echo_factory, make_app};
use app::{MobileAccess, MobileServer};
use canvas::{AgentProfile, CanvasNodeKind, CanvasRect};
use gpui::{AppContext, TestAppContext, VisualTestContext};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use storage::{AppStore, MobilePermission, MobileProjectAccess};

use crate::mobile_wire as wire;

/// 真实 WSS 经授权门、后台请求与现有 GPUI 项目路由读取当前卡片，不泄漏另一项目或旧窗口。
///
/// 数据构造：同一隔离 App 中 A/C 获只读授权、B 未授权；A 两张空卡，C 未打开；无 Provider 进程。
/// 执行过程：查目录 → 查 A → 另一安装尝试沿共享路由查 A → 改名/删卡 → 再查 → 拒绝 B → 订阅后切到 C → 接收 A 关闭事件。
/// 预期结果：无越权内容；A的关闭事件真实到达，关项目后无旧卡片和route；读取不执行任务。
#[test]
fn e2e_mobile_snapshot_uses_actual_window_and_authorized_scope() {
    check_mobile_snapshot_scope();
}

/// 专用本机性能检查保留150ms门槛，不与全仓并行编译/语音/多窗口用例混测。
///
/// 数据构造：20次独立GPUI owner/SQLite/TLS/WSS场景；每次A/C隔离，无Provider任务。
///   20个样本使nearest-rank的p50为第10项、p95为第19项；不丢弃冷启动或最慢样本。
/// 执行过程：连接握手 → Ping/Pong → 订阅首帧 → 切换项目 → 关闭事件；各阶段用同机Instant计时。
/// 预期结果：全部权限/内容断言通过；每次更新小于50ms突发合并+100ms本机余量=150ms。
///   分别输出原始样本、p50/p95/max；不把TCP/TLS/WS握手合计声称为纯TLS耗时，
///   不用回环Rust客户端代替SwiftUI、物理LAN/Tailscale或模型生成性能。
#[test]
#[ignore = "dedicated local latency check; run without concurrent build/test load"]
fn e2e_mobile_project_change_latency() {
    let samples: Vec<_> = (0..20).map(|_| check_mobile_snapshot_scope()).collect();
    let stages = [
        (
            "tcp_tls_ws_connect",
            samples.iter().map(|s| s.connect).collect::<Vec<_>>(),
        ),
        (
            "websocket_ping_rtt",
            samples.iter().map(|s| s.ping).collect(),
        ),
        (
            "initial_project_snapshot",
            samples.iter().map(|s| s.baseline).collect(),
        ),
        (
            "project_change_to_received",
            samples.iter().map(|s| s.change).collect(),
        ),
    ];
    let mut metrics = serde_json::Map::new();
    for (name, values) in stages {
        let raw: Vec<_> = values.iter().map(|v| v.as_secs_f64() * 1000.0).collect();
        let mut ordered = raw.clone();
        ordered.sort_by(f64::total_cmp);
        metrics.insert(
            name.into(),
            json!({"samples_ms":raw,
            "p50_ms":ordered[9], "p95_ms":ordered[18], "max_ms":ordered[19]}),
        );
    }
    eprintln!(
        "MOBILE_LATENCY_REPORT={}",
        json!({
            "scope":"loopback Rust WSS client + headless GPUI + SQLite",
            "sample_count":20, "percentile_method":"nearest-rank", "metrics":metrics,
            "model_generation":"not measured; no Provider task", "idle_cpu_and_wire_bytes":"not measured"
        })
    );
    let worst = samples.iter().map(|s| s.change).max().expect("20次实测");
    assert!(
        worst < Duration::from_millis(150),
        "每个本机事件均不应固定等待250ms或2s心跳，最慢样本: {worst:?}"
    );
}

struct LatencySample {
    connect: Duration,
    ping: Duration,
    baseline: Duration,
    change: Duration,
}

fn check_mobile_snapshot_scope() -> LatencySample {
    let mut cx = TestAppContext::single();
    let socket_dir = tempfile::tempdir().expect("隔离桌面路由");
    cx.update(|cx| {
        app::install_norma_endpoint_for_e2e(socket_dir.path().join("norma.sock"), cx)
            .expect("与实际启动相同的路由注册表")
    });
    let (app, cx) = make_app(&mut cx, echo_factory());
    let a = tempfile::tempdir().expect("A");
    let b = tempfile::tempdir().expect("B");
    let c = tempfile::tempdir().expect("C");
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.enter_canvas_and_register_norma_route_for_e2e(a.path(), window, cx)
        })
    });
    let (path, source, codex, claude) = app.update(cx, |app, cx| {
        let rect = CanvasRect::new(0.0, 0.0, 640.0, 400.0);
        let codex = app
            .runtime
            .create_mobile_agent_card(AgentProfile::Codex, None, None, rect)
            .expect("Codex");
        let claude = app
            .runtime
            .create_mobile_agent_card(AgentProfile::Claude, None, None, rect)
            .expect("Claude");
        (
            app.app_database_path_for_e2e(),
            app.mobile_snapshot_source_for_e2e(cx).expect("正式 owner"),
            codex,
            claude,
        )
    });
    let store = AppStore::new(path);
    let ids = store.workspace_identity_store();
    let a_id = ids
        .resolve_or_create(a.path(), "Project A")
        .expect("A 身份")
        .workspace_id;
    let b_id = ids
        .resolve_or_create(b.path(), "Private B")
        .expect("B 身份")
        .workspace_id;
    let c_id = ids
        .resolve_or_create(c.path(), "Project C")
        .expect("C 身份")
        .workspace_id;
    store
        .mobile_access_store()
        .register(
            "phone-a",
            "iPhone",
            &"a".repeat(64),
            &[
                MobileProjectAccess {
                    workspace_id: a_id.clone(),
                    access: MobilePermission::Read,
                },
                MobileProjectAccess {
                    workspace_id: c_id.clone(),
                    access: MobilePermission::Read,
                },
            ],
        )
        .expect("测试中预置只读授权");
    let identity = norma::mobile::ServerIdentity::generate().expect("TLS");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let server = MobileServer::start_with_snapshots(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        Some(source),
    )
    .expect("WSS");
    let catalog = fetch(cx, &server, &cert, &machine, None);
    assert_eq!(
        catalog["result"]["projects"]
            .as_array()
            .expect("项目目录")
            .len(),
        2,
        "只显示 A 和 C"
    );
    assert!(
        !catalog.to_string().contains(&b_id),
        "未授权 B 身份不得出现在任何字段"
    );
    assert!(
        !catalog
            .to_string()
            .contains(&a.path().display().to_string()),
        "目录不泄漏本地文件路径"
    );
    let detail = fetch(cx, &server, &cert, &machine, Some(&a_id));
    let p = &detail["result"]["projects"][0];
    assert_eq!(p["state"], "open", "A 必须经实际 GPUI 窗口被识别为打开");
    assert!(p["route_id"].as_str().is_some(), "快照必须关联真实窗口实例");
    assert_eq!(p["card_count"], 2, "两张卡都必须出现");
    assert_eq!(
        p["cards"][0]["profile"], "Codex",
        "首卡使用真实 Codex Provider"
    );
    assert_eq!(
        p["cards"][1]["profile"], "Claude",
        "第二卡使用真实 Claude Provider"
    );
    assert_eq!(p["cards"][0]["state"], "idle", "未派任务不能伪造运行");
    // 第二个隔离安装共享 GPUI global route，但拥有不同 App 数据库和 owner。
    // 已知 A 的 ID 也不应穿过 database identity 检查读取另一安装的真实窗口。
    let other =
        cx.update(|_, cx| cx.new(|cx| app::MultisoulApp::new_with_pty_factory(cx, echo_factory())));
    let other_source = other.read_with(cx, |other, cx| {
        other
            .mobile_snapshot_source_for_e2e(cx)
            .expect("另一安装 owner")
    });
    let (reply, result) = crossbeam_channel::bounded(1);
    // 与生产前台投递相同的 2s 上限，逐次让 GPUI 处理队列；无 Provider 任务。
    let deadline = Instant::now() + Duration::from_secs(2);
    other_source
        .try_send(app::LiveSnapshotRequest {
            projects: vec![norma::mobile::ProjectSnapshot {
                workspace_id: a_id.clone(),
                display_name: "Project A".into(),
                state: "unavailable".into(),
                route_id: None,
                card_count: None,
                cards: None,
            }],
            include_cards: true,
            deadline,
            reply,
        })
        .expect("投递另一安装读取");
    let isolated = loop {
        cx.run_until_parked();
        if let Ok(projects) = result.try_recv() {
            break projects;
        }
        assert!(Instant::now() < deadline, "另一安装快照必须有界返回");
    };
    assert_eq!(isolated[0].state, "closed", "A 在另一安装中没有可读取窗口");
    assert!(isolated[0].cards.is_none(), "共享路由不能泄漏 A 的两张卡片");
    assert!(
        isolated[0].route_id.is_none(),
        "另一安装不能获得 A 的窗口身份"
    );
    app.update(cx, |app, _| {
        if let CanvasNodeKind::Agent(spec) =
            &mut app.runtime.document.node_mut(codex).expect("Codex").kind
        {
            spec.alias = Some("Updated title".into());
        }
        app.runtime.remove_node(claude);
    });
    let changed = fetch(cx, &server, &cert, &machine, Some(&a_id));
    assert_eq!(
        changed["result"]["projects"][0]["cards"][0]["title"], "Updated title",
        "按需读取必须取得当前数据"
    );
    assert_eq!(
        changed["result"]["projects"][0]["card_count"], 1,
        "删除的 Claude 不得残留"
    );
    let denied = fetch(cx, &server, &cert, &machine, Some(&b_id));
    assert_eq!(
        denied["error"], "unauthorized",
        "即使知道 B 的 ID 也不能越权读取"
    );
    assert!(denied.get("result").is_none(), "拒绝时不能附带部分项目数据");
    let address = server.addresses()[0];
    let certificate = cert.clone();
    let identity = machine.clone();
    let watched_workspace = a_id.clone();
    let (events, received) = std::sync::mpsc::sync_channel(1);
    let watch = std::thread::spawn(move || {
        let connect_started = Instant::now();
        let mut socket = wire::connect(address, certificate);
        let connect = connect_started.elapsed();
        let ping_started = Instant::now();
        socket
            .send(tungstenite::Message::Ping(b"latency".to_vec().into()))
            .expect("已建连Ping");
        assert_eq!(
            socket.read().expect("Pong"),
            tungstenite::Message::Pong(b"latency".to_vec().into()),
            "必须收到本次Ping的准确Pong，不能将其他帧计作RTT"
        );
        let ping = ping_started.elapsed();
        let baseline_started = Instant::now();
        let first = wire::exchange(
            &mut socket,
            &identity,
            "latency",
            json!({
                "type":"watch_project", "device_id":"phone-a", "token":"a".repeat(64), "workspace_id":watched_workspace
            }),
        );
        let baseline = baseline_started.elapsed();
        events.send((first, Instant::now())).expect("发送基线");
        loop {
            let value: Value =
                serde_json::from_str(socket.read().expect("项目事件").to_text().expect("JSON"))
                    .expect("事件JSON");
            let closed = value["result"]["event"]["project"]["state"] == "closed";
            events.send((value, Instant::now())).expect("发送事件");
            if closed {
                break;
            }
        }
        (connect, ping, baseline)
    });
    let (baseline, _) = next_watch_frame(cx, &received);
    assert_eq!(
        baseline["result"]["event"]["project"]["state"], "open",
        "测量前必须建立真实已打开项目基线"
    );
    let started = Instant::now();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.enter_canvas_and_register_norma_route_for_e2e(c.path(), window, cx)
        })
    });
    let arrived = loop {
        let (event, time) = next_watch_frame(cx, &received);
        if event["result"]["event"]["project"]["state"] == "closed" {
            break time;
        }
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "持续心跳不能代替实际关闭事件"
        );
    };
    let (connect, ping, baseline) = watch.join().expect("读取线程结束");
    let latency = arrived.duration_since(started);
    eprintln!(
        "MOBILE_PROJECT_CHANGE_LATENCY_MS={:.2}",
        latency.as_secs_f64() * 1000.0
    );
    let closed = fetch(cx, &server, &cert, &machine, Some(&a_id));
    assert_eq!(
        closed["result"]["projects"][0]["state"], "closed",
        "离开项目即失去旧窗口路由"
    );
    assert!(
        closed["result"]["projects"][0]["cards"].is_null(),
        "关闭后不能展示旧运行快照"
    );
    assert!(
        closed["result"]["projects"][0]["route_id"].is_null(),
        "旧窗口身份不能沿用"
    );
    assert_eq!(
        app.read_with(cx, |app, _| app.runtime.live_len()),
        0,
        "手机读取不应启动任何执行器"
    );
    LatencySample {
        connect,
        ping,
        baseline,
        change: latency,
    }
}

fn next_watch_frame(
    cx: &mut VisualTestContext,
    receiver: &std::sync::mpsc::Receiver<(Value, Instant)>,
) -> (Value, Instant) {
    //3s与真实WSS客户端读取预算一致，2ms仅驱动测试GPUI，不影响生产调度。
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        cx.run_until_parked();
        if let Ok(event) = receiver.try_recv() {
            return event;
        }
        assert!(Instant::now() < deadline, "项目流必须返回真实事件");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn fetch(
    cx: &mut VisualTestContext,
    server: &MobileServer,
    cert: &[u8],
    machine: &str,
    workspace: Option<&str>,
) -> Value {
    fetch_action(
        cx,
        server,
        cert,
        machine,
        json!({
            "type":"snapshot", "device_id":"phone-a", "token":"a".repeat(64), "workspace_id":workspace
        }),
    )
}

pub(crate) fn fetch_action(
    cx: &mut VisualTestContext,
    server: &MobileServer,
    cert: &[u8],
    machine: &str,
    action: Value,
) -> Value {
    let address = server.addresses()[0];
    let certificate = cert.to_vec();
    let machine = machine.to_string();
    // Reviews can spend2s on each route check plus10s capturing;18s includes4s delivery/scheduling.
    // Other operations retain their original3s socket budget.
    let response_timeout = Duration::from_secs(if action["type"] == "project_review" {
        18
    } else if action["type"] == "archived_conversation" {
        //12s read budget +3s transport margin, matching the native archive client.
        15
    } else {
        3
    });
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let child = std::thread::spawn(move || {
        let mut socket = wire::connect(address, certificate);
        socket
            .get_mut()
            .sock
            .set_read_timeout(Some(response_timeout))
            .expect("有界响应时间");
        let value = wire::exchange(&mut socket, &machine, "snapshot-test", action);
        tx.send(value).expect("回传响应");
    });
    // Add2s for test thread startup and delivery of the already-bounded socket result.
    let deadline = Instant::now() + response_timeout + Duration::from_secs(2);
    let response = loop {
        // 20ms mock-time steps drive the production persistence debounce while real WSS runs.
        cx.cx
            .background_executor
            .advance_clock(Duration::from_millis(20));
        cx.run_until_parked();
        if let Ok(response) = rx.try_recv() {
            break response;
        }
        assert!(Instant::now() < deadline, "网络读取必须有界完成");
        std::thread::sleep(Duration::from_millis(2));
    };
    child.join().expect("网络客户端应正常结束");
    response
}
