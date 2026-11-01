#![cfg(unix)]
use norma::agent_hook::QuestionHookStatus;
use norma::codex::native_questions::{CodexNativeEvent, CodexQuestionHost};
use norma::codex::relay::CodexQuestionRelay;
use serde_json::{json, Value};
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::{Duration, Instant};
use tungstenite::WebSocket;
#[path = "support/codex_relay.rs"]
mod support;
use support::{read, send, timed};

fn connection() -> (
    WebSocket<UnixStream>,
    WebSocket<UnixStream>,
    CodexQuestionRelay,
) {
    connection_mode(false)
}

fn connection_mode(
    paused: bool,
) -> (
    WebSocket<UnixStream>,
    WebSocket<UnixStream>,
    CodexQuestionRelay,
) {
    let dir = tempfile::Builder::new()
        .prefix("nqc-")
        .tempdir_in("/tmp")
        .unwrap();
    let path = dir.path().join("server.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let (native, bridge) = UnixStream::pair().unwrap();
    let worker = std::thread::spawn(move || {
        if paused {
            CodexQuestionRelay::connect_verified_paused(bridge, &path, false).unwrap()
        } else {
            CodexQuestionRelay::connect(bridge, &path).unwrap()
        }
    });
    let server = tungstenite::accept(timed(listener.accept().unwrap().0)).unwrap();
    let native = tungstenite::client("ws://localhost/", timed(native))
        .unwrap()
        .0;
    (native, server, worker.join().unwrap())
}

fn next(host: &CodexQuestionHost) -> CodexNativeEvent {
    host.events().recv_timeout(Duration::from_secs(5)).unwrap()
}

fn ask(server: &mut WebSocket<UnixStream>, native: &mut WebSocket<UnixStream>, thread: &str) {
    for fixture in [
        include_str!("fixtures/questions/codex-source.json"),
        include_str!("fixtures/questions/codex-request.json"),
    ] {
        let value: Value =
            serde_json::from_str(&fixture.replace("codex-question-test-thread", thread)).unwrap();
        send(server, value.clone());
        assert_eq!(read(native), value, "来源和原题必须原样到达原生 TUI");
    }
}

/// 场景：选择器使用辅助连接，真正恢复线程后才转交问答归属。
/// 构造：独立原生/服务端 WebSocket，主连接有待答原题，另建选择器与恢复连接。
/// 执行：选择器查询后关闭；恢复连接取得 thread/resume 成功，再产生新问题。
/// 预期：关闭选择器不使主宿主失效；恢复取消旧票据，新题绑定新连接；旧 TUI 仍能透传。
#[test]
fn picker_disconnect_preserves_owner_and_resume_transfers_question_identity() {
    let (mut native, mut server, relay) = connection();
    let (incoming, connections) = crossbeam_channel::bounded(8);
    let host = CodexQuestionHost::start_with_connections(relay, connections, 7, "owned-run".into())
        .unwrap();
    send(
        &mut native,
        json!({"id":"start","method":"thread/start","params":{}}),
    );
    assert_eq!(
        read(&mut server)["method"],
        "thread/start",
        "由原 TUI 创建原线程"
    );
    send(
        &mut server,
        json!({"id":"start","result":{"thread":{"id":"codex-question-test-thread"}}}),
    );
    read(&mut native);
    let CodexNativeEvent::Attached(original) = next(&host) else {
        panic!("必须先附着原线程")
    };
    ask(&mut server, &mut native, "codex-question-test-thread");
    assert!(
        matches!(next(&host), CodexNativeEvent::SessionUsed(_)),
        "原题标记线程有历史"
    );
    let CodexNativeEvent::Question(old_ticket) = next(&host) else {
        panic!("必须取得原题")
    };

    let (mut picker, mut picker_server, picker_relay) = connection();
    incoming.send(picker_relay).unwrap();
    send(
        &mut picker,
        json!({"id":"list","method":"thread/list","params":{}}),
    );
    assert_eq!(
        read(&mut picker_server)["method"],
        "thread/list",
        "选择器查询必须独立透传"
    );
    send(
        &mut picker_server,
        json!({"id":"list","result":{"data":[]}}),
    );
    assert_eq!(read(&mut picker)["id"], "list", "选择器收到自身查询响应");
    drop(picker);
    drop(picker_server);
    assert!(
        host.events()
            .recv_timeout(Duration::from_millis(100))
            .is_err(),
        "选择器关闭不能发布主连接失效或切换事件"
    );
    assert!(host.is_available(), "关闭辅助连接后原问答宿主仍可用");
    assert_eq!(
        old_ticket.status(Instant::now()),
        QuestionHookStatus::Pending,
        "辅助连接不能取消原题"
    );

    let (mut resumed, mut resumed_server, resumed_relay) = connection();
    incoming.send(resumed_relay).unwrap();
    send(
        &mut resumed,
        json!({"id":"resume","method":"thread/resume","params":{"threadId":"resumed-thread"}}),
    );
    assert_eq!(
        read(&mut resumed_server)["method"],
        "thread/resume",
        "只有原 TUI 请求恢复"
    );
    send(
        &mut resumed_server,
        json!({"id":"resume","result":{"thread":{"id":"resumed-thread"}}}),
    );
    read(&mut resumed);
    let CodexNativeEvent::Attached(current) = next(&host) else {
        panic!("恢复必须发布新附着")
    };
    assert_eq!(
        current.thread_id(),
        "resumed-thread",
        "归属转到实际恢复的线程"
    );
    assert_ne!(
        current.connection_id(),
        original.connection_id(),
        "恢复不能沿用旧连接身份"
    );
    assert_eq!(
        old_ticket.status(Instant::now()),
        QuestionHookStatus::Cancelled,
        "转移归属必须撤销旧题"
    );
    ask(&mut resumed_server, &mut resumed, "resumed-thread");
    assert!(
        matches!(next(&host), CodexNativeEvent::SessionUsed(_)),
        "新题属于恢复线程"
    );
    let CodexNativeEvent::Question(ticket) = next(&host) else {
        panic!("恢复后必须仍能接收原生问题")
    };
    assert_eq!(
        ticket.adapter().connection_id("owned-run"),
        current.connection_id(),
        "新问题绑定恢复连接"
    );
    assert!(host.is_available(), "恢复成功后问答宿主继续可用");

    send(
        &mut native,
        json!({"id":"old-ping","method":"thread/list","params":{}}),
    );
    assert_eq!(
        read(&mut server)["id"],
        "old-ping",
        "归属转移不能主动断开仍存活的原生连接"
    );
    drop(resumed);
    drop(resumed_server);
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut unavailable = false;
    while Instant::now() < deadline {
        if matches!(next(&host), CodexNativeEvent::Unavailable) {
            unavailable = true;
            break;
        }
    }
    assert!(unavailable, "实际拥有者断开必须报告不可用");
    assert!(!host.is_available(), "辅助旧连接不能掩盖实际拥有者失效");
}

/// 场景：问题帧已到达原 TUI 时，自动任务必须立即看到输入锁，不能等宿主下次轮询。
/// 构造：真实 relay 与多连接宿主，32 个不同 turn 的原生请求/完成帧，均不提交模型任务。
/// 执行：逐帧读到 TUI 后立即查询输入状态；32 轮覆盖 5ms 轮询周期内的多个时点。
/// 预期：原题到达时已阻塞，完成帧到达时已解除；不以 sleep 等待采样掩盖竞态。
#[test]
fn native_input_lock_is_live_when_question_frame_reaches_tui() {
    let (mut native, mut server, relay) = connection();
    let (_incoming, connections) = crossbeam_channel::bounded(8);
    let host = CodexQuestionHost::start_with_connections(relay, connections, 7, "input-run".into())
        .unwrap();
    send(
        &mut native,
        json!({"id":"start","method":"thread/start","params":{}}),
    );
    read(&mut server);
    send(
        &mut server,
        json!({"id":"start","result":{"thread":{"id":"input-thread"}}}),
    );
    read(&mut native);
    assert!(
        matches!(next(&host), CodexNativeEvent::Attached(_)),
        "先建立当前原生线程归属"
    );
    for index in 0..32 {
        let turn = format!("turn-{index}");
        let pending = json!({"id":index,"method":"item/tool/requestUserInput","params":{
            "threadId":"input-thread","turnId":turn,"itemId":format!("item-{index}"),"questions":[]
        }});
        send(&mut server, pending.clone());
        assert_eq!(read(&mut native), pending, "原生问题先到达 TUI");
        assert!(
            host.input_pending(),
            "第 {index} 轮问题到达后必须立即阻止自动任务"
        );
        let completed = json!({"method":"turn/completed","params":{"threadId":"input-thread","turn":{"id":turn}}});
        send(&mut server, completed.clone());
        assert_eq!(read(&mut native), completed, "完成帧到达 TUI");
        assert!(!host.input_pending(), "第 {index} 轮完成后实时解除输入锁");
    }
}

/// 场景：新连接尚未交付 Attached 事件时，选择器和恢复后的问题也必须占用输入。
/// 构造：原主连接空闲，新连接使用生产的暂停转发握手；一次恢复后立即跟随问题帧。
/// 执行：登记新连接、initialize、resume、问题；读到 TUI 帧即查询，不等待宿主事件。
/// 预期：选择器开始转发前已禁止普通输入；恢复问题无轮询窗口，完成后实时解除。
#[test]
fn new_connection_holds_input_before_forwarding_or_attachment_delivery() {
    let (mut native, mut server, relay) = connection();
    let (incoming, connections) = crossbeam_channel::bounded(8);
    let host =
        CodexQuestionHost::start_with_connections(relay, connections, 7, "handover-run".into())
            .unwrap();
    send(
        &mut native,
        json!({"id":"start","method":"thread/start","params":{}}),
    );
    read(&mut server);
    send(
        &mut server,
        json!({"id":"start","result":{"thread":{"id":"idle-thread"}}}),
    );
    read(&mut native);
    assert!(
        matches!(next(&host), CodexNativeEvent::Attached(_)),
        "原主连接先附着"
    );
    assert!(
        !host.input_pending(),
        "原主连接空闲，不能掩盖新连接的输入窗口"
    );

    let (mut resumed, mut resumed_server, relay) = connection_mode(true);
    incoming.send(relay).unwrap();
    send(
        &mut resumed,
        json!({"id":"init","method":"initialize","params":{}}),
    );
    read(&mut resumed_server);
    send(
        &mut resumed_server,
        json!({"id":"init","result":{"userAgent":"codex-tui/0.162.1 test"}}),
    );
    read(&mut resumed);
    assert!(host.input_pending(), "选择器已开始转发时必须先占用输入");

    send(
        &mut resumed,
        json!({"id":"resume","method":"thread/resume","params":{"threadId":"next-thread"}}),
    );
    read(&mut resumed_server);
    send(
        &mut resumed_server,
        json!({"id":"resume","result":{"thread":{"id":"next-thread"}}}),
    );
    let pending = json!({"id":3,"method":"item/tool/requestUserInput","params":{
        "threadId":"next-thread","turnId":"next-turn","itemId":"next-item","questions":[]
    }});
    send(&mut resumed_server, pending.clone());
    assert_eq!(read(&mut resumed)["id"], "resume", "先收到恢复结果");
    assert_eq!(read(&mut resumed), pending, "问题帧已到达新 TUI");
    assert!(
        host.input_pending(),
        "未等待 Attached 也必须阻止自动文字进入新问题"
    );
    assert!(
        matches!(next(&host), CodexNativeEvent::Attached(_)),
        "随后交付新连接归属"
    );
    let complete = json!({"method":"turn/completed","params":{"threadId":"next-thread","turn":{"id":"next-turn"}}});
    send(&mut resumed_server, complete.clone());
    assert_eq!(read(&mut resumed), complete, "完成帧到达新 TUI");
    assert!(
        !host.input_pending(),
        "完成后读当前 relay 的实时标志，不能遗留选择器输入锁"
    );
}
