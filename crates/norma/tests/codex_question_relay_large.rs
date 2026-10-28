#![cfg(unix)]
use norma::codex::relay::CodexQuestionRelay;
use serde_json::json;
use std::os::unix::net::{UnixListener, UnixStream};
use tungstenite::{protocol::WebSocketConfig, Message};
#[path = "support/codex_relay.rs"]
mod support;
use support::{read, send, timed};

/// 场景：本机原生首任务会收到约23MB的合法配置/上下文消息，不能被问答观察器的8MB限制拆断。
/// 构造：自有Unix server向原TUI发送24MiB合成正文（大于实测22,973,488字节），无真实上下文内容。
/// 执行：经生产relay透传原大消息，再从TUI回传小ACK；不创建题目或额外回合。
/// 预期：原帧正文和ID完整；TUI仍能继续通讯。独立题面账本容量不随传输上限扩大。
#[test]
fn native_large_configuration_frame_does_not_break_original_connection() {
    let dir = tempfile::Builder::new()
        .prefix("nql-")
        .tempdir_in("/tmp")
        .unwrap();
    let path = dir.path().join("s.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let (native, bridge) = UnixStream::pair().unwrap();
    let server = std::thread::spawn(move || {
        let mut socket = tungstenite::accept(timed(listener.accept().unwrap().0)).unwrap();
        send(
            &mut socket,
            json!({"id":"original-config","result":{"context":"x".repeat(24*1024*1024)}}),
        );
        assert_eq!(
            read(&mut socket)["method"],
            "continued",
            "原TUI必须还能发后续消息"
        );
        send(&mut socket, json!({"id":"ack","result":{}}));
        let _ = socket.read();
    });
    let tui = std::thread::spawn(move || {
        let config = WebSocketConfig::default()
            .max_message_size(Some(64 * 1024 * 1024))
            .max_frame_size(Some(64 * 1024 * 1024));
        let mut socket =
            tungstenite::client::client_with_config("ws://localhost/", timed(native), Some(config))
                .unwrap()
                .0;
        let value = read(&mut socket);
        assert_eq!(value["id"], "original-config", "大消息身份保持不变");
        assert_eq!(
            value["result"]["context"].as_str().unwrap().len(),
            24 * 1024 * 1024,
            "24MiB原正文不能被截断"
        );
        socket
            .send(Message::Text(
                json!({"id":"ack","method":"continued"}).to_string().into(),
            ))
            .unwrap();
        assert_eq!(read(&mut socket)["id"], "ack", "大消息后原连接仍可往返");
        let _ = socket.close(None);
    });
    let _relay = CodexQuestionRelay::connect(bridge, &path).unwrap();
    tui.join().unwrap();
    server.join().unwrap();
}
