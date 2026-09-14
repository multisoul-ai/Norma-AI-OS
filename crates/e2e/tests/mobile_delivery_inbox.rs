use app::{
    LiveCommandRequest, LiveSnapshotRequest, MobileAccess, MobileCommand, MobileServer,
    MobileSources,
};
use norma::mobile::ServerIdentity;
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use storage::{
    AppStore, ApprovalAuditStore, BackgroundApprovalAudit, MobileCommandStore, MobilePermission,
    MobileProjectAccess, MobileTaskDeliveryStore, MobileTurnKey, MobileTurnOutcome,
    MobileTurnState, MobileTurnStore, TaskDeliveryContent,
};
use workspace::review::ProjectReview;
#[path = "support/mobile.rs"]
mod support;

/// 待验收订阅响应另一手机的正式验收命令，保留身份/权限/窗口边界且只传元数据。
/// 数据构造：reader只读、writer可控制同项目；完成成果a/b共2份，1个后台快照fixture，不运行Agent。
/// 执行过程：订阅2份→空心跳→writer验收b→同流变为1份→reader撤销→writer流窗口失效。
/// 预期结果：验收b后只保留a，总数2→1；心跳不重传列表，撤销和旧窗口不能继续读。
/// 1s = 50ms通知合并 + 950ms本地调度余量，小于2s心跳，保证正式命令会唤醒订阅。
#[test]
fn delivery_inbox_stream_tracks_acceptance_without_polling_or_scope_leaks() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let db = dir.path().join("app.sqlite");
    let store = AppStore::new(db.clone());
    let workspace = store
        .workspace_identity_store()
        .resolve_or_create(dir.path(), "验收项目")
        .expect("项目")
        .workspace_id;
    for (device, permission) in [
        ("reader", MobilePermission::Read),
        ("writer", MobilePermission::Control),
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
            .expect("限定项目授权");
    }
    let writer = BackgroundApprovalAudit::new(
        ApprovalAuditStore::new(dir.path().join("audit.sqlite")).with_turn_store(
            MobileTurnStore::for_workspace(db.clone(), workspace.clone()),
        ),
    )
    .expect("正式终态writer");
    let deliveries = MobileTaskDeliveryStore::new(db.clone());
    let key = |turn: &str| MobileTurnKey {
        node_id: 7,
        profile: "Codex".into(),
        run_id: "run".into(),
        session_id: "session".into(),
        turn_id: turn.into(),
    };
    for turn in ["a", "b"] {
        // 5s沿用后台writer的提交预算，只接受commit回执。
        writer
            .record_turn(MobileTurnOutcome {
                summary: None,
                key: key(turn),
                state: MobileTurnState::Completed,
            })
            .expect("入队")
            .recv_timeout(Duration::from_secs(5))
            .expect("回执")
            .expect("commit");
        deliveries
            .record(
                &workspace,
                &key(turn),
                &TaskDeliveryContent::Available {
                    review: ProjectReview {
                        base_commit: None,
                        conflicted: false,
                        files: vec![],
                    },
                },
            )
            .expect("冻结成果");
    }
    let b = deliveries
        .get(&workspace, &key("b"))
        .expect("读取b")
        .expect("存在");
    let (snapshots, requests) = async_channel::bounded::<LiveSnapshotRequest>(8);
    let changed = Arc::new(AtomicBool::new(false));
    let route = changed.clone();
    let owner = std::thread::spawn(move || {
        while let Ok(mut call) = requests.recv_blocking() {
            call.projects[0].state = "open".into();
            call.projects[0].route_id = Some(
                if route.load(Ordering::SeqCst) {
                    "new-route"
                } else {
                    "route"
                }
                .into(),
            );
            call.projects[0].card_count = Some(0);
            call.projects[0].cards = Some(vec![]);
            let _ = call.reply.send(call.projects);
        }
    });
    let (commands, requests) = async_channel::bounded::<LiveCommandRequest>(1);
    let executor = std::thread::spawn(move || {
        while let Ok(call) = requests.recv_blocking() {
            assert!(call.lease.begin(), "不能绕过授权租约");
            let MobileCommand::AcceptDelivery { key, fingerprint } = &call.request.command else {
                panic!("只接受指定成果验收");
            };
            let result = MobileCommandStore::new(db.clone())
                .accept_delivery(&call.source, key, fingerprint)
                .expect("正式持久化验收");
            call.reply
                .send(Ok(result.result()))
                .expect("命令完成触发既有变化信号");
        }
    });
    let identity = ServerIdentity::generate().expect("随机身份");
    let machine = identity.machine_id();
    let cert = identity.certificate_der().to_vec();
    let server = MobileServer::start_with_mobile_sources(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        MobileSources {
            snapshots: Some(snapshots.clone()),
            commands: Some(commands.clone()),
            ..Default::default()
        },
    )
    .expect("正式服务");
    let action = |device: &str| json!({"type":"task_deliveries","watch":true,"pending_only":true,"device_id":device,"token":"a".repeat(64),"request":{"workspace_id":workspace,"route_id":"route"}});
    let mut socket = support::connect(server.addresses()[0], cert.clone());
    let initial = support::exchange(&mut socket, &machine, "inbox", action("reader"));
    assert_eq!(
        initial["result"]["status"], "delivery_event",
        "现有成果接口应支持持续订阅"
    );
    let stream = initial["result"]["delivery_event"]["stream_id"].clone();
    let page = &initial["result"]["delivery_event"]["snapshot"]["deliveries"];
    assert_eq!(page["pending_count"], 2, "初始确有2份待验收成果");
    assert_eq!(
        page["items"][0]["outcome"]["key"]["turn_id"], "b",
        "最新成果在前"
    );
    assert!(
        page["items"][0].get("content").is_none(),
        "统计和列表不能携带成果正文"
    );
    // support::connect使用客户端TLS类型，后续读取保留类型推导。
    let heartbeat: Value =
        serde_json::from_str(socket.read().expect("保活").to_text().expect("文本")).expect("JSON");
    assert!(
        heartbeat["result"]["delivery_event"]["snapshot"].is_null(),
        "心跳不重复传输列表"
    );
    socket
        .get_mut()
        .sock
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("事件预算");
    let mut control = support::connect(server.addresses()[0], cert.clone());
    let receipt = support::exchange(
        &mut control,
        &machine,
        "accept",
        json!({"type":"command","device_id":"writer","token":"a".repeat(64),"request":{"operation_id":"accept-b","workspace_id":workspace,"route_id":"route","grant_revision":1,"command":{"type":"accept_delivery","key":key("b"),"fingerprint":b.fingerprint}}}),
    );
    assert_eq!(
        receipt["result"]["status"], "receipt",
        "命令须经过正式回执入口"
    );
    let updated: Value = serde_json::from_str(
        socket
            .read()
            .expect("验收立即推送")
            .to_text()
            .expect("文本"),
    )
    .expect("JSON");
    let event = &updated["result"]["delivery_event"];
    assert_eq!(event["stream_id"], stream, "不靠重连发现验收");
    assert_eq!(event["sequence"], 1, "验收只推进一次真实变化");
    assert_eq!(
        event["snapshot"]["deliveries"]["pending_count"], 1,
        "验收后总数减少"
    );
    assert_eq!(
        event["snapshot"]["deliveries"]["items"][0]["outcome"]["key"]["turn_id"], "a",
        "保留未验收a"
    );
    assert_eq!(
        event["snapshot"]["deliveries"]["items"]
            .as_array()
            .expect("列表")
            .len(),
        1,
        "b不能残留待验收"
    );
    server.revoke("reader").expect("撤销");
    assert!(socket.read().is_err(), "撤销不能继续读");
    let mut scope = support::connect(server.addresses()[0], cert);
    assert_eq!(
        support::exchange(&mut scope, &machine, "scope", action("writer"))["result"]["status"],
        "delivery_event",
        "control也可读取"
    );
    changed.store(true, Ordering::SeqCst);
    let expired: Value =
        serde_json::from_str(scope.read().expect("窗口失效事件").to_text().expect("文本"))
            .expect("JSON");
    assert_eq!(expired["error"], "route_expired", "不能把新窗口当作旧路由");
    snapshots.close();
    commands.close();
    server.shutdown();
    owner.join().expect("回收快照fixture");
    executor.join().expect("回收验收fixture");
}
