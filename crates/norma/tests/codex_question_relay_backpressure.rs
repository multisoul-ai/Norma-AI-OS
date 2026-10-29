#![cfg(unix)]
use norma::codex::relay::CodexQuestionRelay;
use serde_json::json;
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::Duration;
#[path = "support/codex_relay.rs"]
mod support;
use support::{read, send, timed};

use tungstenite::Message;

/// 场景：服务端短暂停止读取，大帧必须受背压而非撑爆relay缓冲并断开原TUI。
/// 构造：48条各512KiB消息共24MiB，超过本机socket队列容量；接收端暂时不消费。
/// 执行：发送端连续写，等待1秒确认尚无错误；放开接收端后逐条核对并回复。
/// 预期：内存有界、连接存活，全部帧保持顺序与内容，观察器不得因此拆掉原生会话。
#[test]
fn slow_server_applies_backpressure_without_closing_the_native_connection() {
    let dir = tempfile::Builder::new()
        .prefix("nqr-")
        .tempdir_in("/tmp")
        .unwrap();
    let path = dir.path().join("server.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let (native, bridge) = UnixStream::pair().unwrap();
    let (release, gate) = crossbeam_channel::bounded(1);
    let (sent, sending) = crossbeam_channel::bounded(1);
    let server = std::thread::spawn(move || {
        let mut socket = tungstenite::accept(timed(listener.accept().unwrap().0)).unwrap();
        gate.recv_timeout(Duration::from_secs(5)).unwrap();
        for index in 0..48 {
            let frame = read(&mut socket);
            assert_eq!(frame["id"], index, "背压不得丢帧或乱序");
            assert_eq!(
                frame["params"]["text"].as_str().unwrap().len(),
                512 * 1024,
                "正文保持完整"
            );
        }
        send(&mut socket, json!({"id":"all","result":{}}));
        let _ = socket.read();
    });
    let tui = std::thread::spawn(move || {
        let mut socket = tungstenite::client("ws://localhost/", timed(native))
            .unwrap()
            .0;
        for index in 0..48 {
            let frame = json!({"id":index,"method":"large/passthrough","params":{"text":"x".repeat(512*1024)}});
            if let Err(error) = socket.send(Message::Text(frame.to_string().into())) {
                sent.send(Err(error.to_string())).unwrap();
                return;
            }
        }
        sent.send(Ok(())).unwrap();
        assert_eq!(read(&mut socket)["id"], "all", "TUI收到完整传输的确认");
        socket.close(None).unwrap();
    });
    let relay = CodexQuestionRelay::connect(bridge, &path).unwrap();
    let early = sending.recv_timeout(Duration::from_secs(1));
    assert!(
        !matches!(early, Ok(Err(_))),
        "等待服务端读取时不能错误关闭连接：{early:?}"
    );
    assert!(relay.is_running(), "下游背压不能杀死原生连接");
    release.send(()).unwrap();
    if early.is_err() {
        sending
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
    }
    tui.join().unwrap();
    server.join().unwrap();
}
