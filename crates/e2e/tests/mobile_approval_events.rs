use app::{LiveApprovalRequest, LiveSnapshotRequest, MobileAccess, MobileServer, MobileSources};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use storage::{AppStore, MobilePermission, MobileProjectAccess};

#[path = "support/mobile.rs"]
mod wire;

/// 一条电脑级WSS汇总全部已授权项目审批，心跳不重发工具正文，撤权立即断流。
///
/// 数据构造：A打开且有1个Claude审批，C已关闭，B未授权；手机只有read权限；无Provider进程。
///   2s沿用正式授权心跳；测试socket3s读预算足以覆盖一次心跳，不据此宣称低延迟通过。
/// 执行过程：订阅 → 基线/心跳 → 读审批时空卡数量1→2，保留同一审批 → 清空 → 读中关项目 → 重连 → 撤销。
/// 预期结果：A准确工具输入存在，C无审批，B不泄漏；读到旧输入后路由已变也不能回传，撤销断流。
#[test]
fn approval_feed_scopes_projects_and_removes_resolved_requests() {
    check_feed();
}

/// 单独测量已建立审批流的事件唤醒，不能等待2s心跳或旧3s前台轮询。
/// 数据构造：复用同一授权隔离场景；fixture先清空请求，再经另一测试手机撤销唤醒已有共享信号。
/// 执行过程：记录本机Instant → 共享变更信号 → 正式授权/快照/审批读取 → WSS接收清空事件。
/// 预期结果：原手机仍保持revision1且旧审批消失；耗时<50ms合并+100ms本机余量=150ms。
/// 范围：回环Rust客户端和前台数据fixture，不代表真实Provider、SwiftUI或跨网延迟。
#[test]
#[ignore = "dedicated local approval event latency; run without concurrent builds/tests"]
fn approval_feed_local_latency() {
    let elapsed = check_feed();
    assert!(
        elapsed < Duration::from_millis(150),
        "审批变化不能等待旧轮询周期：{elapsed:?}"
    );
}

fn check_feed() -> Duration {
    let dir = tempfile::tempdir().expect("隔离目录");
    let store = AppStore::new(dir.path().join("app.db"));
    let a = store
        .workspace_identity_store()
        .resolve_or_create(dir.path(), "A")
        .expect("A")
        .workspace_id;
    let cdir = tempfile::tempdir().expect("关闭项目");
    let c = store
        .workspace_identity_store()
        .resolve_or_create(cdir.path(), "C")
        .expect("C")
        .workspace_id;
    let bdir = tempfile::tempdir().expect("私有项目");
    let b = store
        .workspace_identity_store()
        .resolve_or_create(bdir.path(), "Private B")
        .expect("B")
        .workspace_id;
    store
        .mobile_access_store()
        .register(
            "phone",
            "iPhone",
            &"a".repeat(64),
            &[
                MobileProjectAccess {
                    workspace_id: a.clone(),
                    access: MobilePermission::Read,
                },
                MobileProjectAccess {
                    workspace_id: c.clone(),
                    access: MobilePermission::Read,
                },
            ],
        )
        .expect("只读授权");
    store
        .mobile_access_store()
        .register(
            "wake",
            "Unused test phone",
            &"b".repeat(64),
            &[MobileProjectAccess {
                workspace_id: a.clone(),
                access: MobilePermission::Read,
            }],
        )
        .expect("仅用于唤醒共享信号的隔离设备");
    let resolved = Arc::new(AtomicBool::new(false));
    let closed = Arc::new(AtomicBool::new(false));
    let close_during_read = Arc::new(AtomicBool::new(false));
    let card_count = Arc::new(AtomicUsize::new(1));
    let change_metadata = Arc::new(AtomicBool::new(false));
    let (snapshots, sr) = async_channel::bounded::<LiveSnapshotRequest>(8);
    let open = a.clone();
    let route_closed = closed.clone();
    let snapshot_count = card_count.clone();
    let snapshot_worker = std::thread::spawn(move || {
        while let Ok(mut request) = sr.recv_blocking() {
            assert!(!request.include_cards, "汇总审批不应复制整项目卡片内容");
            for project in &mut request.projects {
                if project.workspace_id == open && !route_closed.load(Ordering::Acquire) {
                    project.state = "open".into();
                    project.route_id = Some("route-a".into());
                    project.card_count = Some(snapshot_count.load(Ordering::Acquire));
                } else {
                    project.state = "closed".into();
                }
            }
            let _ = request.reply.send(request.projects);
        }
    });
    let (approvals, ar) = async_channel::bounded::<LiveApprovalRequest>(8);
    let completed = resolved.clone();
    let closing = close_during_read.clone();
    let metadata_change = change_metadata.clone();
    let expected = a.clone();
    let approval_worker = std::thread::spawn(move || {
        while let Ok(request) = ar.recv_blocking() {
            assert_eq!(
                request.request.workspace_id, expected,
                "只有打开的授权A能请求审批"
            );
            assert_eq!(
                request.request.route_id, "route-a",
                "必须从当前项目路由读取"
            );
            let items = if completed.load(Ordering::Acquire) {
                vec![]
            } else {
                vec![serde_json::from_value(json!({
            "target":{"node_id":7,"profile":"Claude","run_id":"run-a","session_id":"session-a","turn_id":"turn-a","request_id":"request-a"},
            "card_title":"Agent", "tool":"Bash", "details":{"command":"printf exact"}, "decisions":["allow","deny"]
        })).expect("准确审批fixture")]
            };
            if closing.load(Ordering::Acquire) {
                closed.store(true, Ordering::Release);
            }
            if metadata_change.load(Ordering::Acquire) {
                card_count.store(2, Ordering::Release);
            }
            let _ = request.reply.send(Ok(items));
        }
    });
    let identity = norma::mobile::ServerIdentity::generate().expect("TLS");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let server = MobileServer::start_with_mobile_sources(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        MobileSources {
            snapshots: Some(snapshots),
            approvals: Some(approvals),
            ..Default::default()
        },
    )
    .expect("WSS");
    let mut socket = wire::connect(server.addresses()[0], cert.clone());
    let first = wire::exchange(
        &mut socket,
        &machine,
        "feed",
        json!({"type":"watch_approvals","device_id":"phone","token":"a".repeat(64)}),
    );
    assert_eq!(
        first["result"]["status"], "approval_event",
        "必须支持电脑级实时订阅"
    );
    let event = &first["result"]["approval_event"];
    assert_eq!(event["sequence"], 0, "新连接必须从零序号完整基线开始");
    let snapshot = &event["snapshot"];
    assert_eq!(
        snapshot["grant"]["projects"][0]["access"], "read",
        "实时读取不能升权"
    );
    let projects = snapshot["projects"].as_array().expect("全部授权项目");
    assert_eq!(
        projects.len(),
        2,
        "A和C都必须有明确状态，不能省略已关闭项目"
    );
    let live = projects
        .iter()
        .find(|p| p["project"]["workspace_id"] == a)
        .expect("A");
    let closed = projects
        .iter()
        .find(|p| p["project"]["workspace_id"] == c)
        .expect("C");
    assert_eq!(
        live["items"][0]["details"]["command"], "printf exact",
        "必须保留真实工具输入"
    );
    assert_eq!(closed["items"], json!([]), "关闭项目不能残留可操作审批");
    assert!(!first.to_string().contains(&b), "未授权B不能出现在事件中");
    let heartbeat = read(&mut socket);
    assert_eq!(
        heartbeat["result"]["approval_event"]["sequence"], 0,
        "无变化不能虚构新序号"
    );
    assert!(
        heartbeat["result"]["approval_event"]["snapshot"].is_null(),
        "心跳不重复工具正文"
    );
    change_metadata.store(true, Ordering::Release);
    let metadata = read(&mut socket);
    assert_eq!(
        metadata["result"]["approval_event"]["sequence"], 1,
        "路由相同的卡片数量变化不能断流或重新从零开始"
    );
    let sources = metadata["result"]["approval_event"]["snapshot"]["projects"]
        .as_array()
        .expect("更新后的完整状态");
    let live = sources
        .iter()
        .find(|p| p["project"]["workspace_id"] == a)
        .expect("A仍在");
    assert_eq!(
        live["project"]["card_count"], 2,
        "必须采用最终核验的当前元数据"
    );
    assert_eq!(
        live["items"][0]["details"]["command"], "printf exact",
        "不能因增加空卡而丢弃有效审批"
    );
    resolved.store(true, Ordering::Release);
    let started = Instant::now();
    server
        .revoke("wake")
        .expect("同一公开撤销路径唤醒共享信号，不改变被测手机授权");
    let changed = read(&mut socket);
    let elapsed = started.elapsed();
    eprintln!(
        "APPROVAL_FEED_CHANGE_MS={:.3}",
        elapsed.as_secs_f64() * 1000.0
    );
    assert_eq!(
        changed["result"]["approval_event"]["snapshot"]["grant"]["revision"], 1,
        "另一设备变动只能唤醒，不能污染当前设备权限"
    );
    assert_eq!(
        changed["result"]["approval_event"]["sequence"], 2,
        "实际清空推进一次序号"
    );
    assert!(
        !changed.to_string().contains("printf exact"),
        "已解决请求不能继续展示旧输入"
    );
    assert_eq!(
        changed["result"]["approval_event"]["snapshot"]["projects"][0]["items"],
        json!([]),
        "审批清空必须发完整新状态"
    );
    resolved.store(false, Ordering::Release);
    close_during_read.store(true, Ordering::Release);
    let late = read(&mut socket);
    assert_eq!(
        late["error"], "unavailable",
        "读取期间项目路由改变必须丢弃整份旧审批"
    );
    assert!(
        late.get("result").is_none(),
        "路由失效不能夹带部分旧工具输入"
    );
    drop(socket);
    let mut socket = wire::connect(server.addresses()[0], cert);
    let closed = wire::exchange(
        &mut socket,
        &machine,
        "fresh",
        json!({"type":"watch_approvals","device_id":"phone","token":"a".repeat(64)}),
    );
    assert_eq!(
        closed["result"]["approval_event"]["sequence"], 0,
        "重连不能沿用旧序号"
    );
    assert!(
        !closed.to_string().contains("printf exact"),
        "重连后不复活关闭项目的输入"
    );
    server.revoke("phone").expect("撤销");
    assert!(
        socket.read().is_err(),
        "撤销立即关闭已标记订阅，不得再输出正文"
    );
    drop(socket);
    drop(server);
    snapshot_worker.join().expect("快照线程回收");
    approval_worker.join().expect("审批线程回收");
    elapsed
}

fn read(
    socket: &mut tungstenite::WebSocket<
        rustls::StreamOwned<rustls::ClientConnection, std::net::TcpStream>,
    >,
) -> Value {
    serde_json::from_str(
        socket
            .read()
            .expect("有界实时事件")
            .to_text()
            .expect("文本帧"),
    )
    .expect("事件JSON")
}
