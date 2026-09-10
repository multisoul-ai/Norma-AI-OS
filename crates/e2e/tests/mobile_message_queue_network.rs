use app::{MobileAccess, MobileServer};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use storage::{AppStore, MobilePermission, MobileProjectAccess};

#[path = "support/mobile.rs"]
mod support;

/// 正式 WSS 队列必须先持久化，忙碌时不输入；撤回不派发，空闲后仅领取一次并等待真实格式回执。
///
/// 数据构造：一个临时项目、phone-a control revision=1；node7/run-a/session-a；快照用可控原生通道模拟，TLS和存储真实。
/// 执行过程：忙碌入队 → 取消第二条/拒绝插队 → Ready后发送首条 → 新条目输入前拒绝回队 → 输入后失联标记未知 → 降权取消。
/// 预期结果：queued不立即发；取消条目不执行；dispatching不能安全撤回；仅明确输入前拒绝可重试，输入后未知不重放。
#[test]
fn wss_queue_waits_cancels_and_dispatches_once_with_scoped_receipts() {
    let directory = tempfile::tempdir().expect("目录");
    let store = AppStore::new(directory.path().join("app.sqlite"));
    let workspace = store
        .workspace_identity_store()
        .resolve_or_create(directory.path(), "Queue")
        .expect("项目")
        .workspace_id;
    store
        .mobile_access_store()
        .register(
            "phone-a",
            "phone",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: workspace.clone(),
                access: MobilePermission::Control,
            }],
        )
        .expect("授权");
    let identity = norma::mobile::ServerIdentity::generate().expect("证书");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let (snapshots, snapshot_requests) = async_channel::bounded::<app::LiveSnapshotRequest>(8);
    let (commands, deliveries) = async_channel::bounded(8);
    let ready = Arc::new(AtomicBool::new(false));
    let fixture_ready = ready.clone();
    let fixture = std::thread::spawn(move || {
        while let Ok(mut request) = snapshot_requests.recv_blocking() {
            for project in &mut request.projects {
                project.state = "open".into();
                project.route_id = Some("route-a".into());
                project.card_count = Some(1);
                project.cards = request
                    .include_cards
                    .then(|| vec![card(fixture_ready.load(Ordering::Acquire))]);
            }
            let _ = request.reply.send(request.projects);
        }
    });
    let server = MobileServer::start_with_sources(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        Some(snapshots),
        Some(commands),
    )
    .expect("服务");
    let mut socket = support::connect(server.addresses()[0], cert.clone());
    let action = |operation: &str, text: &str| {
        json!({"type":"command","device_id":"phone-a","token":"a".repeat(64),
        "request":{"operation_id":operation,"workspace_id":workspace,"route_id":"route-a","grant_revision":1,
        "command":{"type":"queue_message","expected":card(false),"text":text}}})
    };
    let query = |operation: &str| json!({"type":"queued_message","device_id":"phone-a","token":"a".repeat(64),"workspace_id":workspace,"operation_id":operation});
    let cancel = |operation: &str| json!({"type":"cancel_queued_message","device_id":"phone-a","token":"a".repeat(64),"workspace_id":workspace,"operation_id":operation,"grant_revision":1});
    let first = action("first", "first message");
    let ack = support::exchange(&mut socket, &machine, "enqueue-first", first.clone());
    assert_eq!(
        ack["result"]["receipt"]["result"]["status"], "queued",
        "必须取得已持久化的入队回执"
    );
    assert_eq!(
        support::exchange(
            &mut socket,
            &machine,
            "enqueue-second",
            action("second", "cancel me")
        )["result"]["receipt"]["state"],
        "succeeded",
        "第二条独立保存"
    );
    assert_eq!(
        support::exchange(&mut socket, &machine, "retry-first", first.clone())["result"]["receipt"]
            ["result"]["queue_id"],
        "first",
        "重复只返回原入队"
    );
    assert_eq!(
        support::exchange(&mut socket, &machine, "read-first", query("first"))["result"]
            ["queued_message"]["state"],
        "queued",
        "忙碌时应等待"
    );
    assert!(deliveries.try_recv().is_err(), "忙碌时不能向前台输入");
    assert_eq!(
        support::exchange(&mut socket, &machine, "cancel-second", cancel("second"))["result"]
            ["queued_message"]["state"],
        "cancelled",
        "未提交条目可以明确取消"
    );
    let mut overtake = action("overtake", "must not overtake");
    overtake["request"]["command"]["type"] = json!("send_message");
    overtake["request"]["command"]["expected"] = json!(card(true));
    assert_eq!(
        support::exchange(&mut socket, &machine, "overtake", overtake)["error"],
        "queue_pending",
        "新直接发送不可越过同卡已接受的队列"
    );
    ready.store(true, Ordering::Release);
    // 5s = background queue wake ≤500ms + foreground snapshot ≤2s + scheduling margin.
    let deadline = Instant::now() + Duration::from_secs(5);
    let call = loop {
        if let Ok(call) = deliveries.try_recv() {
            break call;
        }
        assert!(Instant::now() < deadline, "空闲后应有界领取队列");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(call.request.operation_id, "first", "只能领取未取消的第一条");
    assert!(
        matches!(&call.request.command, app::MobileCommand::SendMessage { text, .. } if text == "first message"),
        "调度必须复用准确发送入口"
    );
    assert!(call.lease.begin(), "有效授权可开始一次");
    assert_eq!(
        support::exchange(&mut socket, &machine, "late-cancel", cancel("first"))["error"],
        "queue_not_cancellable",
        "开始提交后不得伪装安全撤回"
    );
    call.reply
        .send(Ok(
            json!({"node_id":7,"run_id":"run-a","session_id":"session-a","turn_id":"turn-b"}),
        ))
        .expect("模拟准确Provider确认");
    let sent = loop {
        let value = support::exchange(&mut socket, &machine, "read-sent", query("first"));
        if value["result"]["queued_message"]["state"] == "sent" {
            break value;
        }
        assert!(Instant::now() < deadline, "准确回执应落盘");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(
        sent["result"]["queued_message"]["result"]["turn_id"], "turn-b",
        "发送状态绑定新回合"
    );
    assert_eq!(
        support::exchange(&mut socket, &machine, "repeat-done", first)["result"]["receipt"]
            ["result"]["queue_id"],
        "first",
        "完成后重试不产生新消息"
    );
    assert!(
        deliveries.try_recv().is_err(),
        "取消条目和已发送首项都不能再次派发"
    );
    let deferred = action("deferred", "keep original");
    assert_eq!(
        support::exchange(&mut socket, &machine, "enqueue-deferred", deferred.clone())["result"]
            ["receipt"]["result"]["status"],
        "queued",
        "下一条先持久化"
    );
    let rejected = wait_for(|| deliveries.try_recv().ok());
    ready.store(false, Ordering::Release);
    rejected
        .reply
        .send(Err("desktop_input_pending"))
        .expect("明确未输入");
    wait_for(|| {
        let value = support::exchange(&mut socket, &machine, "deferred-state", query("deferred"));
        (value["result"]["queued_message"]["state"] == "queued").then_some(())
    });
    assert!(
        deliveries.try_recv().is_err(),
        "电脑有输入时不得持续重复投递"
    );
    drop(rejected);
    ready.store(true, Ordering::Release);
    let uncertain = wait_for(|| deliveries.try_recv().ok());
    assert_eq!(
        uncertain.request.operation_id, "deferred",
        "仅原条目恢复输入前的等待"
    );
    assert!(uncertain.lease.begin(), "新领取的有效lease可开始一次");
    drop(uncertain); // No reply after beginning: the dispatcher cannot prove whether input happened.
    let unknown = wait_for(|| {
        let value = support::exchange(&mut socket, &machine, "unknown-state", query("deferred"));
        (value["result"]["queued_message"]["state"] == "outcome_unknown").then_some(value)
    });
    assert_eq!(
        unknown["result"]["queued_message"]["text"], "keep original",
        "不确定结果仍保留原文"
    );
    assert!(
        unknown["result"]["queued_message"]["result"].is_null(),
        "不得伪造Provider收到"
    );
    assert_eq!(
        support::exchange(&mut socket, &machine, "retry-unknown", deferred)["result"]["receipt"]
            ["result"]["status"],
        "queued",
        "重复只返回历史入队回执"
    );
    assert_eq!(
        support::exchange(&mut socket, &machine, "cancel-unknown", cancel("deferred"))["error"],
        "queue_not_cancellable",
        "输入后失联不能安全撤回"
    );
    assert!(
        deliveries.try_recv().is_err(),
        "未知条目不能因回查/重试被重放"
    );
    ready.store(false, Ordering::Release);
    assert_eq!(
        support::exchange(
            &mut socket,
            &machine,
            "enqueue-revoked",
            action("revoked", "must stay unsent")
        )["result"]["receipt"]["result"]["status"],
        "queued",
        "降权前已接收的消息先等待"
    );
    // Downgrade to read revision=2, then prove the read endpoint works without granting queue writes.
    server
        .replace_permissions(
            "phone-a",
            1,
            &[MobileProjectAccess {
                workspace_id: workspace.clone(),
                access: MobilePermission::Read,
            }],
        )
        .expect("降权");
    let mut socket = support::connect(server.addresses()[0], cert);
    let revoked_deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let revoked = support::exchange(&mut socket, &machine, "read-revoked", query("revoked"));
        if revoked["result"]["queued_message"]["state"] == "cancelled" {
            break;
        }
        assert!(
            Instant::now() < revoked_deadline,
            "权限变化必须使已排队消息失效"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(deliveries.try_recv().is_err(), "旧授权的队列不得继续执行");
    let mut denied = action("readonly", "must not run");
    denied["request"]["grant_revision"] = json!(2);
    assert_eq!(
        support::exchange(&mut socket, &machine, "read-only", denied)["error"],
        "unauthorized",
        "只读不能入队"
    );
    assert_eq!(
        support::exchange(&mut socket, &machine, "still-readable", query("first"))["result"]
            ["queued_message"]["state"],
        "sent",
        "只读可查询自己的既有结果"
    );
    let mut foreign = query("first");
    foreign["workspace_id"] = json!("other-project");
    assert_eq!(
        support::exchange(&mut socket, &machine, "foreign", foreign)["error"],
        "unauthorized",
        "队列正文不能跨项目泄漏"
    );
    drop(socket);
    drop(call);
    drop(server);
    fixture.join().expect("后台快照线程应随server退出");
}

fn card(ready: bool) -> norma::mobile::CardSnapshot {
    serde_json::from_value::<norma::mobile::CardSnapshot>(json!({"node_id":7,"profile":"Claude","title":"Queue","mobile_control":true,
        "state":if ready { "ready" } else { "thinking" },"run_id":"run-a","session_id":"session-a","turn_id":if ready { Value::Null } else { json!("turn-a") },"task_title":null})).expect("受控快照")
}

fn wait_for<T>(mut read: impl FnMut() -> Option<T>) -> T {
    // 5s = one 500ms queue wake + 2s snapshot budget + 2.5s scheduling allowance.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(value) = read() {
            return value;
        }
        assert!(Instant::now() < deadline, "队列状态转换应在有界预算内完成");
        // 10ms samples only this controlled fixture; it is not a production polling rate.
        std::thread::sleep(Duration::from_millis(10));
    }
}
