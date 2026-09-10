use app::{LiveCommandRequest, LiveSnapshotRequest, MobileAccess, MobileServer};
use norma::mobile::{CardSnapshot, ServerIdentity};
use serde_json::{json, Value};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use storage::{AppStore, MobilePermission, MobileProjectAccess, ProjectPlanStore};
#[path = "support/mobile.rs"]
mod support;

/// 真实WSS对Todo按项目授权，一次决定关联既有发送/队列，不从手机接受替换指令。
///
/// 数据构造：plan-a有三项指向node7/run-a；A控制、B控制、R只读；前台fixture返回精确发送回合。
/// 终态fixture：首项completed、排队项failed，均经后台SQLite落盘；拒绝项无终态。
/// 执行过程：R拒绝写 → A接受0 → B重复拒绝 → A拒绝1 → 排队2并撤回 → B显式重排 → 旧预览拒绝 → 派发/撤销。
/// 预期结果：仅两次原文SendMessage、一次拒绝无副作用；共享状态含准确回执、无跨项目泄露。
/// 实时验收：R以只读订阅基线，跨过3×2s心跳；A/B的决定、撤回和终态经同一WSS到达。
///   变更读取预算1s=50ms合并+950ms调度余量，小于2s保活，防止通知遗漏被轮询掩盖。
#[test]
fn todo_decisions_dispatch_original_text_once_and_share_scoped_status() {
    let dir = tempfile::tempdir().expect("隔离数据");
    let store = AppStore::new(dir.path().join("app.db"));
    let workspace = store
        .workspace_identity_store()
        .resolve_or_create(dir.path(), "Todo")
        .expect("项目")
        .workspace_id;
    let card = CardSnapshot {
        node_id: 7,
        profile: "Codex".into(),
        title: "测试".into(),
        mobile_control: true,
        can_interrupt: None,
        task_timing: None,
        task_progress: None,
        task_summary: None,
        state: "ready".into(),
        run_id: Some("run-a".into()),
        session_id: Some("session-a".into()),
        turn_id: None,
        task_title: None,
    };
    let draft = agent::ProjectPlanDraft {
        summary: "下一步".into(),
        todos: vec![
            agent::PlanTodo {
                target: card.clone(),
                title: "一".into(),
                instruction: "第一项原文".into(),
            },
            agent::PlanTodo {
                target: card.clone(),
                title: "二".into(),
                instruction: "拒绝项原文".into(),
            },
            agent::PlanTodo {
                target: card.clone(),
                title: "三".into(),
                instruction: "排队项原文".into(),
            },
        ],
    };
    ProjectPlanStore::new(store.path().into())
        .publish("plan-a", &workspace, "route-a", "验证边界", &draft)
        .expect("建议");
    for (device, permission) in [
        ("phone-a", MobilePermission::Control),
        ("phone-b", MobilePermission::Control),
        ("phone-r", MobilePermission::Read),
    ] {
        store
            .mobile_access_store()
            .register(
                device,
                device,
                &"a".repeat(64),
                &[MobileProjectAccess {
                    workspace_id: workspace.clone(),
                    access: permission,
                }],
            )
            .expect("授权");
    }
    let identity = ServerIdentity::generate().expect("TLS");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let (snapshots, reads) = async_channel::bounded::<LiveSnapshotRequest>(8); // Matches the eight-peer server cap.
    let (commands, calls) = async_channel::bounded::<LiveCommandRequest>(8);
    let current = Arc::new(Mutex::new(card.clone()));
    let cards = current.clone();
    let reader = std::thread::spawn(move || {
        while let Ok(mut call) = reads.recv_blocking() {
            for project in &mut call.projects {
                project.state = "open".into();
                project.route_id = Some("route-a".into());
                project.card_count = Some(1);
                project.cards = call
                    .include_cards
                    .then(|| vec![cards.lock().expect("卡片").clone()]);
            }
            let _ = call.reply.send(call.projects);
        }
    });
    let inputs = Arc::new(Mutex::new(Vec::<String>::new()));
    let sent = inputs.clone();
    let outcome_writer = storage::BackgroundApprovalAudit::new(
        storage::ApprovalAuditStore::new(dir.path().join("project.sqlite")).with_turn_store(
            storage::MobileTurnStore::for_workspace(store.path().into(), workspace.clone()),
        ),
    )
    .expect("后台回合证据fixture");
    let dispatcher = std::thread::spawn(move || {
        while let Ok(call) = calls.recv_blocking() {
            assert!(call.lease.begin(), "必须有正式授权租约");
            if let app::MobileCommand::SendMessage { expected, text } = &call.request.command {
                assert_eq!(expected.node_id, 7, "只能派给指定卡片");
                sent.lock().expect("发送记录").push(text.clone());
                let state = if text == "第一项原文" {
                    storage::MobileTurnState::Completed
                } else {
                    storage::MobileTurnState::Failed
                };
                let outcome = storage::MobileTurnOutcome {
                    key: storage::MobileTurnKey {
                        node_id: 7,
                        profile: "Codex".into(),
                        run_id: "run-a".into(),
                        session_id: "session-a".into(),
                        turn_id: format!("turn-{}", call.request.operation_id),
                    },
                    state,
                };
                // 5s = 原有本地writer提交预算；此fixture模拟终态，真实Provider另有原生检查。
                outcome_writer
                    .record_turn(outcome)
                    .expect("终态入队")
                    .recv_timeout(Duration::from_secs(5))
                    .expect("终态回执")
                    .expect("终态提交");
                let _=call.reply.send(Ok(json!({"node_id":7,"run_id":"run-a","session_id":"session-a","turn_id":format!("turn-{}",call.request.operation_id)})));
            } else {
                panic!("Todo应复用正式SendMessage，而不是新的执行器");
            }
        }
    });
    let server = MobileServer::start_with_sources(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        Some(snapshots),
        Some(commands),
    )
    .expect("WSS");
    let mut watch = support::connect(server.addresses()[0], cert.clone());
    let subscription = json!({"type":"watch_plan","device_id":"phone-r","token":"a".repeat(64),"workspace_id":workspace,"plan_id":"plan-a"});
    let baseline = support::exchange(&mut watch, &machine, "watch", subscription.clone());
    assert_eq!(
        baseline["result"]["status"], "plan_event",
        "必须支持计划实时订阅"
    );
    let initial = &baseline["result"]["plan_event"];
    assert_eq!(initial["sequence"], 0, "初始完整快照从零开始");
    assert_eq!(
        initial["snapshot"]["decisions"],
        json!([]),
        "初始不能虚构已决定项"
    );
    assert_eq!(
        initial["snapshot"]["grant"]["projects"][0]["access"], "read",
        "订阅不能升权"
    );
    let stream = initial["stream_id"].as_str().expect("订阅身份").to_owned();
    let mut sequence = 0;
    for _ in 0..3 {
        let heartbeat: Value =
            serde_json::from_str(watch.read().expect("持续保活").to_text().expect("文本"))
                .expect("JSON");
        let event = &heartbeat["result"]["plan_event"];
        assert_eq!(event["sequence"], 0, "心跳不推进序号");
        assert!(event["snapshot"].is_null(), "空闲不能重发计划全文");
    }
    watch
        .get_mut()
        .sock
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("变更应在下一次心跳前到达");
    let mut socket = support::connect(server.addresses()[0], cert.clone());
    let accept = json!({"type":"accept","target":card,"queue":false});
    let action = |device: &str, operation: &str, index: usize, choice: Value| json!({"type":"command","device_id":device,"token":"a".repeat(64),"request":{"operation_id":operation,"workspace_id":workspace,"route_id":"route-a","grant_revision":1,"command":{"type":"decide_plan_todo","plan_id":"plan-a","todo_index":index,"choice":choice}}});
    let denied = support::exchange(
        &mut socket,
        &machine,
        "r",
        action("phone-r", "no-write", 0, accept.clone()),
    );
    assert_eq!(denied["error"], "unauthorized", "只读不能安排任务");
    let request = action("phone-a", "send-a", 0, accept.clone());
    let accepted = support::exchange(&mut socket, &machine, "a", request.clone());
    assert_eq!(
        accepted["result"]["status"], "receipt",
        "接受必须有持久化回执"
    );
    // 5s = up to2s foreground validation +30ms fixture send + transport/scheduling margin.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let r = support::exchange(
            &mut socket,
            &machine,
            "receipt",
            json!({"type":"receipt","device_id":"phone-a","token":"a".repeat(64),"workspace_id":workspace,"operation_id":"send-a"}),
        );
        if r["result"]["receipt"]["state"] == "succeeded" {
            break;
        }
        assert!(Instant::now() < deadline, "发送回执必须有界完成");
    }
    let duplicate = support::exchange(
        &mut socket,
        &machine,
        "b",
        action("phone-b", "send-b", 0, json!({"type":"reject"})),
    );
    assert_eq!(
        duplicate["error"], "todo_decided",
        "另一手机不能覆盖已决定项"
    );
    let replay = support::exchange(&mut socket, &machine, "again", request);
    assert_eq!(
        replay["result"]["receipt"]["result"]["turn_id"], "turn-send-a",
        "原请求只返回原回合"
    );
    let decline = support::exchange(
        &mut socket,
        &machine,
        "decline",
        action("phone-a", "reject-a", 1, json!({"type":"reject"})),
    );
    assert_eq!(
        decline["result"]["receipt"]["result"]["status"], "declined",
        "拒绝只完成决定"
    );
    let changed = watch_until(&mut watch, &stream, &mut sequence, |s| {
        s["decisions"][1]["choice"]["type"] == "reject"
    });
    assert_eq!(
        changed["decisions"][0]["outcome"]["state"], "completed",
        "实时状态包含准确落盘回合"
    );
    assert!(
        changed["decisions"][1]["outcome"].is_null(),
        "拒绝通知不能虚构执行"
    );
    current.lock().expect("忙碌").state = "thinking".into();
    let mut busy = card.clone();
    busy.state = "thinking".into();
    let queued = support::exchange(
        &mut socket,
        &machine,
        "queue",
        action(
            "phone-a",
            "queue-a",
            2,
            json!({"type":"accept","target":busy,"queue":true}),
        ),
    );
    assert_eq!(
        queued["result"]["receipt"]["result"]["status"], "queued",
        "忙碌时持久化到已有队列"
    );
    assert_eq!(
        *inputs.lock().expect("输入"),
        vec!["第一项原文"],
        "拒绝项和忙碌项不得发送"
    );
    let query_retry = json!({"type":"project_plan","device_id":"phone-b","token":"a".repeat(64),"workspace_id":workspace,"plan_id":"plan-a"});
    let cancelled = support::exchange(
        &mut socket,
        &machine,
        "cancel",
        json!({"type":"cancel_queued_message","device_id":"phone-a","token":"a".repeat(64),"workspace_id":workspace,"operation_id":"queue-a","grant_revision":1}),
    );
    assert_eq!(
        cancelled["result"]["queued_message"]["state"], "cancelled",
        "未发送队列应明确撤回"
    );
    let changed = watch_until(&mut watch, &stream, &mut sequence, |s| {
        s["decisions"][2]["queued_message"]["state"] == "cancelled"
    });
    assert_eq!(
        changed["decisions"][2]["can_retry"], true,
        "撤回必须唤醒订阅并允许显式重排"
    );
    assert!(
        changed["decisions"][2]["outcome"].is_null(),
        "撤回项没有终态"
    );
    let cancelled_status = support::exchange(
        &mut socket,
        &machine,
        "cancelled-status",
        query_retry.clone(),
    );
    assert_eq!(
        cancelled_status["result"]["decisions"][2]["can_retry"], true,
        "只有明确未发送才显示重新安排"
    );
    let mut retry = action(
        "phone-b",
        "retry-queue",
        2,
        json!({"type":"accept","target":busy,"queue":true}),
    );
    retry["request"]["command"]["retry_of"] =
        json!({"device_id":"phone-a","operation_id":"queue-a"});
    let accepted_retry = support::exchange(&mut socket, &machine, "retry", retry.clone());
    assert_eq!(
        accepted_retry["result"]["receipt"]["result"]["status"], "queued",
        "另一控制手机可明确重新安排原文"
    );
    let repeated = support::exchange(&mut socket, &machine, "retry-again", retry.clone());
    assert_eq!(
        repeated["result"]["receipt"]["operation_id"], "retry-queue",
        "重复只回查这次安排"
    );
    let mut stale = retry.clone();
    stale["request"]["operation_id"] = json!("stale-retry");
    assert_eq!(
        support::exchange(&mut socket, &machine, "stale-retry", stale)["error"],
        "todo_decided",
        "旧决定预览不能再安排一次"
    );
    let old_queue = support::exchange(
        &mut socket,
        &machine,
        "old-queue",
        json!({"type":"queued_message","device_id":"phone-a","token":"a".repeat(64),"workspace_id":workspace,"operation_id":"queue-a"}),
    );
    assert_eq!(
        old_queue["result"]["queued_message"]["state"], "cancelled",
        "旧队列不可被新尝试改写"
    );
    let retried_status = support::exchange(&mut socket, &machine, "retry-status", query_retry);
    assert_eq!(
        retried_status["result"]["decisions"][2]["retry_of"]["operation_id"], "queue-a",
        "新尝试保留原决定身份"
    );
    assert_eq!(
        retried_status["result"]["decisions"][2]["can_retry"], false,
        "新队列仍待发送时不能重复重排"
    );
    current.lock().expect("就绪").state = "ready".into();
    let query = json!({"type":"project_plan","device_id":"phone-b","token":"a".repeat(64),"workspace_id":workspace,"plan_id":"plan-a"});
    let deadline = Instant::now() + Duration::from_secs(5); // 500ms existing dispatcher tick + 2s read + margin.
    let status = loop {
        let r = support::exchange(&mut socket, &machine, "status", query.clone());
        if r["result"]["decisions"][2]["queued_message"]["state"] == "sent" {
            break r;
        }
        assert!(Instant::now() < deadline, "排队任务必须使用已有调度完成");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(
        status["result"]["decisions"][0]["receipt"]["result"]["turn_id"], "turn-send-a",
        "其他获授权设备应看到实际任务身份"
    );
    assert_eq!(
        status["result"]["decisions"][1]["choice"]["type"], "reject",
        "应保留拒绝决定"
    );
    assert_eq!(
        *inputs.lock().expect("输入"),
        vec!["第一项原文", "排队项原文"],
        "只能各发送一次保存的原文"
    );
    assert_eq!(
        status["result"]["decisions"][0]["outcome"]["state"], "completed",
        "已发送项必须关联准确回合的持久化完成证据"
    );
    assert_eq!(
        status["result"]["decisions"][2]["outcome"]["state"], "failed",
        "后续失败必须属于后续Todo"
    );
    let changed = watch_until(&mut watch, &stream, &mut sequence, |s| {
        s["decisions"][2]["queued_message"]["state"] == "sent"
    });
    assert_eq!(
        changed["decisions"][2]["outcome"]["state"], "failed",
        "队列送达后的真实失败不能遗漏"
    );
    assert!(
        status["result"]["decisions"][1]["outcome"].is_null(),
        "拒绝项不能拥有虚构任务终态"
    );
    assert_eq!(
        status["result"]["decisions"][0]["outcome"]["key"]["turn_id"], "turn-send-a",
        "不能借用另一回合结果"
    );
    current.lock().expect("更换当前运行").run_id = Some("replacement-run".into());
    let history = support::exchange(&mut socket, &machine, "historical", query.clone());
    assert_eq!(
        history["result"]["decisions"][0]["outcome"]["key"]["run_id"], "run-a",
        "卡片新运行不能替换旧Todo的终态身份"
    );
    assert_eq!(
        history["result"]["decisions"][2]["outcome"]["state"], "failed",
        "后续卡片状态不能抹掉已存失败"
    );
    let mut hidden = query.clone();
    hidden["workspace_id"] = json!("hidden");
    assert_eq!(
        support::exchange(&mut socket, &machine, "hidden", hidden)["error"],
        "unauthorized",
        "不能跨项目读取决定或原文"
    );
    server.revoke("phone-b").expect("撤销");
    // Revocation closes an authenticated socket; use a fresh socket for the rejection response.
    let mut socket = support::connect(server.addresses()[0], cert.clone());
    assert_eq!(
        support::exchange(&mut socket, &machine, "revoked", query)["error"],
        "unauthorized",
        "撤销后不能读共享计划状态"
    );
    let mut hidden = subscription.clone();
    hidden["workspace_id"] = json!("hidden");
    let denied = support::exchange(&mut socket, &machine, "hidden-watch", hidden);
    assert_eq!(denied["error"], "unauthorized", "实时订阅不能跨项目");
    assert!(denied.get("result").is_none(), "拒绝不得夹带计划正文");
    server.revoke("phone-r").expect("撤销订阅设备");
    assert!(watch.read().is_err(), "撤销必须关闭已标记的订阅连接");
    drop(watch);
    drop(socket);
    server.shutdown();
    drop(server);
    reader.join().expect("读线程");
    dispatcher.join().expect("派发线程");
}

fn watch_until(
    socket: &mut tungstenite::WebSocket<
        rustls::StreamOwned<rustls::ClientConnection, std::net::TcpStream>,
    >,
    stream: &str,
    sequence: &mut u64,
    predicate: impl Fn(&Value) -> bool,
) -> Value {
    let started = Instant::now();
    loop {
        let reply: Value = serde_json::from_str(
            socket
                .read()
                .expect("实时变更，不能等2s保活")
                .to_text()
                .expect("文本"),
        )
        .expect("JSON");
        let event = &reply["result"]["plan_event"];
        assert_eq!(event["stream_id"], stream, "持续连接不能更换流身份");
        if event["snapshot"].is_null() {
            assert_eq!(event["sequence"], *sequence, "心跳不能跳过未收到的序号");
        } else {
            *sequence += 1;
            assert_eq!(event["sequence"], *sequence, "变更序号必须连续");
            if predicate(&event["snapshot"]) {
                eprintln!(
                    "PLAN_EVENT_READ_MS={:.3}",
                    started.elapsed().as_secs_f64() * 1000.0
                );
                return event["snapshot"].clone();
            }
        }
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "不能以无限心跳掩盖遗漏的变更"
        );
    }
}
