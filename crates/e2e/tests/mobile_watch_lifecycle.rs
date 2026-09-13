use app::{
    LiveConversationRequest, LiveSnapshotRequest, LiveTerminalRequest, MobileAccess, MobileServer,
    MobileSources,
};
use norma::mobile::{CardSnapshot, ConversationPage, ServerIdentity, TerminalPage};
use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpStream,
    time::{Duration, Instant},
};
use storage::{AppStore, MobilePermission, MobileProjectAccess};
use tungstenite::Message;
#[path = "support/mobile.rs"]
mod support;

/// 快速离开页面必须结束旧订阅，满额连接下新页面仍能立即连接。
///
/// 数据构造：一个只读手机、一个空计划项目；覆盖计划/项目/审批/成果/聊天/终端，共8个订阅等于正式上限。
///   250ms响应预算 = 50ms变更合并 + 20ms监听调度 + 180ms TLS/线程调度余量，远小于2s心跳。
/// 执行过程：建立8个正式TLS/WSS订阅 → 第9条拒绝 → Ping/Pong → Close/异常断开 → 立即替代，各8次。
/// 预期结果：每次Close及时返回且新订阅收到plan_event；不得等心跳、收到错误或泄露其他项目。
#[test]
fn closed_watches_release_capacity_before_the_heartbeat() {
    let dir = tempfile::tempdir().expect("隔离数据");
    let store = AppStore::new(dir.path().join("app.sqlite"));
    let workspace = store
        .workspace_identity_store()
        .resolve_or_create(dir.path(), "页面切换")
        .expect("项目身份")
        .workspace_id;
    store
        .mobile_access_store()
        .register(
            "phone",
            "只读手机",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: workspace.clone(),
                access: MobilePermission::Read,
            }],
        )
        .expect("只读授权");
    let identity = ServerIdentity::generate().expect("TLS身份");
    let machine = identity.machine_id();
    let certificate = identity.certificate_der().to_vec();
    let card = CardSnapshot {
        node_id: 7,
        profile: "Codex".into(),
        title: "同一卡片".into(),
        mobile_control: true,
        can_interrupt: None,
        task_timing: None,
        task_progress: None,
        task_summary: None,
        state: "ready".into(),
        run_id: Some("run".into()),
        session_id: Some("session".into()),
        turn_id: None,
        task_title: None,
    };
    // 三个队列容量8与正式连接上限相同；只合成快照内容，TLS/鉴权/订阅/断连均使用生产代码。
    let (snapshots, reads) = async_channel::bounded::<LiveSnapshotRequest>(8);
    let projected_card = card.clone();
    let snapshot_worker = std::thread::spawn(move || {
        while let Ok(mut call) = reads.recv_blocking() {
            let p = &mut call.projects[0];
            p.state = "open".into();
            p.route_id = Some("route".into());
            p.card_count = Some(1);
            p.cards = call.include_cards.then_some(vec![projected_card.clone()]);
            let _ = call.reply.send(call.projects);
        }
    });
    let (conversations, histories) = async_channel::bounded::<LiveConversationRequest>(8);
    let history_worker = std::thread::spawn(move || {
        while let Ok(call) = histories.recv_blocking() {
            let page = ConversationPage::from_codex(json!({"data":[],"nextCursor":null}))
                .expect("空会话页");
            let _ = call.reply.send(Ok(page));
        }
    });
    let (terminals, screens) = async_channel::bounded::<LiveTerminalRequest>(8);
    let terminal_worker = std::thread::spawn(move || {
        while let Ok(call) = screens.recv_blocking() {
            let _ = call.reply.send(Ok(TerminalPage {
                card: card.clone(),
                revision: 1,
                screen_signature: 1,
                input_sequence: 0,
                start_line: 0,
                total_lines: 1,
                cols: 80,
                running: true,
                input_ready: true,
                rows: vec!["Ready".into()],
                soft_wrapped: vec![false],
            }));
        }
    });
    let server = MobileServer::start_with_mobile_sources(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("隔离回环")],
        MobileSources {
            snapshots: Some(snapshots),
            conversations: Some(conversations),
            terminals: Some(terminals),
            commands: None,
            approvals: None,
            reviews: None,
        },
    )
    .expect("正式服务");
    let connect = |kind: usize| {
        let mut socket = support::connect(server.addresses()[0], certificate.clone());
        let (mut action, status) = match kind % 6 {
            0 => (
                json!({"type":"watch_plan", "workspace_id":workspace}),
                "plan_event",
            ),
            1 => (
                json!({"type":"watch_project", "workspace_id":workspace}),
                "project_event",
            ),
            2 => (json!({"type":"watch_approvals"}), "approval_event"),
            3 => (
                json!({"type":"task_deliveries", "watch":true, "request":{"workspace_id":workspace,"route_id":"route"}}),
                "delivery_event",
            ),
            4 => (
                json!({"type":"watch_conversation", "request":{"workspace_id":workspace,"route_id":"route","node_id":7,"run_id":"run","session_id":"session"}}),
                "conversation_event",
            ),
            _ => (
                json!({"type":"terminal", "watch":true, "request":{"workspace_id":workspace,"route_id":"route","node_id":7,"run_id":"run","session_id":"session"}}),
                "terminal_event",
            ),
        };
        action["device_id"] = json!("phone");
        action["token"] = json!("a".repeat(64));
        let reply = support::exchange(&mut socket, &machine, "watch", action);
        assert_eq!(reply["result"]["status"], status, "新页面必须成功订阅");
        assert!(reply.get("error").is_none(), "不能用错误帧冒充订阅基线");
        assert!(
            reply["result"]["plan_event"]["snapshot"]["plan"].is_null(),
            "空项目不得借用其他计划"
        );
        socket
    };
    let mut sockets: Vec<_> = (0..8).map(&connect).collect();
    let mut ninth = TcpStream::connect(server.addresses()[0]).expect("满额时连接监听端口");
    ninth
        .set_read_timeout(Some(Duration::from_millis(250)))
        .expect("拒绝预算");
    assert_eq!(
        ninth.read(&mut [0; 1]).expect("拒绝多余连接"),
        0,
        "真实8条连接上限必须仍生效"
    );
    // 掩码[9,8,7,6]异或负载[1,2,3]得到[8,10,4]；先写7/9字节，延迟余下2字节。
    // 80ms > 50ms合并周期，<2s心跳，验证半帧不会被当成断连或提前产生Pong。
    let partial = &mut sockets[0];
    partial
        .get_mut()
        .sock
        .set_read_timeout(Some(Duration::from_millis(80)))
        .expect("半帧观察预算");
    partial
        .get_mut()
        .write_all(&[0x89, 0x83, 9, 8, 7, 6, 8])
        .expect("TLS内发送部分masked Ping");
    partial.get_mut().flush().expect("刷新部分帧");
    assert!(
        matches!(partial.read(), Err(tungstenite::Error::Io(error)) if matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut)),
        "半帧必须保持连接等待完整内容，不返回业务数据或Close"
    );
    partial.get_mut().write_all(&[10, 4]).expect("补齐Ping");
    partial.get_mut().flush().expect("刷新完整帧");
    partial
        .get_mut()
        .sock
        .set_read_timeout(Some(Duration::from_millis(250)))
        .expect("完整帧响应预算");
    assert_eq!(
        partial.read().expect("半帧补齐后Pong"),
        Message::Pong(vec![1, 2, 3].into()),
        "TLS/WS缓冲必须保留完整原始负载"
    );
    let mut close_times = Vec::new();
    for cycle in 0..16 {
        let mut previous = sockets.remove(0);
        previous
            .get_mut()
            .sock
            .set_read_timeout(Some(Duration::from_millis(250)))
            .expect("短关闭预算");
        previous
            .send(Message::Ping(vec![1, 2, 3].into()))
            .expect("订阅中保活");
        assert_eq!(
            previous.read().expect("250ms内收到Pong"),
            Message::Pong(vec![1, 2, 3].into()),
            "服务必须读取准确Ping并回Pong，不泄漏业务内容"
        );
        let began = Instant::now();
        if cycle >= 8 {
            previous
                .get_mut()
                .sock
                .shutdown(std::net::Shutdown::Both)
                .expect("模拟系统取消/切网断开");
            drop(previous);
            sockets.push(connect(cycle));
            assert!(
                began.elapsed() < Duration::from_millis(250),
                "异常断开后新页面也不得等心跳"
            );
            continue;
        }
        previous.close(None).expect("手机发出Close");
        let reply = previous.read();
        assert!(
            matches!(reply, Ok(Message::Close(_))),
            "第{cycle}轮服务必须及时回应Close，实际{reply:?}"
        );
        assert!(
            began.elapsed() < Duration::from_millis(250),
            "旧连接不能占用至2秒心跳"
        );
        close_times.push(began.elapsed().as_micros());
        drop(previous);
        sockets.push(connect(cycle));
    }
    let mut invalid = sockets.remove(0);
    invalid
        .send(Message::text("unexpected request on watch"))
        .expect("订阅中不允许第二业务请求");
    let refusal: serde_json::Value = serde_json::from_str(
        invalid
            .read()
            .expect("明确拒绝")
            .to_text()
            .expect("错误JSON"),
    )
    .expect("解析拒绝");
    assert_eq!(
        refusal["error"], "invalid_request",
        "订阅不能接收额外业务操作"
    );
    assert!(
        refusal.get("result").is_none(),
        "无效帧不得派发命令或返回结果"
    );
    eprintln!("close_ack_us={close_times:?}");
    server.shutdown();
    drop(server);
    snapshot_worker.join().expect("回收项目快照线程");
    history_worker.join().expect("回收聊天快照线程");
    terminal_worker.join().expect("回收终端快照线程");
}
