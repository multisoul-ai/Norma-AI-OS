#![cfg(any(unix, windows))]
use norma::codex::relay::CodexQuestionRelay;
use serde_json::json;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::Duration;
use tungstenite::{protocol::Role, Message, WebSocket};
#[cfg(windows)]
use uds_windows::{UnixListener, UnixStream};

fn timed(stream: UnixStream) -> UnixStream {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
}

/// 场景：非阻塞 listener 接受的流分段收到 WebSocket 握手，不能把暂缺字节当作失败。
/// 构造：原生流显式非阻塞，先写请求行，100ms 后补齐合法 HTTP 头；上游为真实本地 socket。
/// 执行：后台调用生产 relay，确认它等待剩余头；补齐握手后往返 thread/list 并关闭。
/// 预期：握手不提前退出，原 RPC 正确返回，双方在 5s 有界 IO 内结束；不要求凭据或模型。
#[test]
fn fragmented_handshake_on_nonblocking_accepted_stream_waits_for_the_rest() {
    let dir = tempfile::Builder::new()
        .prefix("nch-")
        .tempdir_in(if cfg!(windows) {
            std::env::temp_dir()
        } else {
            "/tmp".into()
        })
        .unwrap();
    let path = dir.path().join("up.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let native_path = dir.path().join("tui.sock");
    let native_listener = UnixListener::bind(&native_path).unwrap();
    native_listener.set_nonblocking(true).unwrap();
    let mut native = UnixStream::connect(&native_path).unwrap();
    let accepted = native_listener.accept().unwrap().0;
    accepted.set_nonblocking(true).unwrap();
    native.write_all(b"GET / HTTP/1.1\r\n").unwrap();
    let (up, upstream) = crossbeam_channel::bounded(1);
    let server = std::thread::spawn(move || -> Result<(), String> {
        let mut socket =
            tungstenite::accept(timed(listener.accept().map_err(|e| e.to_string())?.0))
                .map_err(|e| e.to_string())?;
        up.send(()).unwrap();
        let request = socket.read().map_err(|e| e.to_string())?;
        let value: serde_json::Value =
            serde_json::from_str(request.to_text().map_err(|e| e.to_string())?).unwrap();
        assert_eq!(value["id"], 7, "分段握手后仍使用同一原 RPC 身份");
        socket
            .send(Message::Text(
                json!({"id":7,"result":{"data":[]}}).to_string().into(),
            ))
            .map_err(|e| e.to_string())?;
        let _ = socket.read();
        Ok(())
    });
    let (sent, result) = crossbeam_channel::bounded(1);
    let worker = std::thread::spawn(move || {
        sent.send(CodexQuestionRelay::connect(accepted, &path))
            .unwrap()
    });
    upstream.recv_timeout(Duration::from_secs(5)).unwrap();
    // 已写入请求行；补齐前的 100ms 是“不可提前失败”的观察窗，不是网络吞吐假设。
    let early = result.recv_timeout(Duration::from_millis(100));
    if let Ok(Err(reason)) = &early {
        drop(native);
        worker.join().unwrap();
        let _ = server.join().unwrap();
        panic!("非阻塞接收流必须等到握手完整，实际提前失败：{reason}");
    }
    let native_result = (|| -> Result<(), String> {
        let mut native = timed(native);
        native.write_all(b"Host: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n").map_err(|e|e.to_string())?;
        let mut response = Vec::new();
        while !response.ends_with(b"\r\n\r\n") && response.len() < 8192 {
            let mut byte = [0];
            native.read_exact(&mut byte).map_err(|e| e.to_string())?;
            response.push(byte[0]);
        }
        assert!(
            response.starts_with(b"HTTP/1.1 101"),
            "握手必须成功升级，不能返回普通 HTTP"
        );
        let mut socket = WebSocket::from_raw_socket(native, Role::Client, None);
        socket
            .send(Message::Text(
                json!({"id":7,"method":"thread/list","params":{}})
                    .to_string()
                    .into(),
            ))
            .map_err(|e| e.to_string())?;
        let frame = socket.read().map_err(|e| e.to_string())?;
        let reply: serde_json::Value =
            serde_json::from_str(frame.to_text().map_err(|e| e.to_string())?).unwrap();
        assert_eq!(
            reply,
            json!({"id":7,"result":{"data":[]}}),
            "请求响应不能因分段握手丢失"
        );
        let _ = socket.close(None);
        Ok(())
    })();
    let waited = early.is_err();
    let connected = early.unwrap_or_else(|_| result.recv_timeout(Duration::from_secs(5)).unwrap());
    worker.join().unwrap();
    let upstream_result = server.join().unwrap();
    assert!(
        waited,
        "握手字节暂未到达时必须等待，实际：{:?}",
        connected.as_ref().err()
    );
    assert!(
        connected.is_ok(),
        "分段握手必须成功：{:?}",
        connected.as_ref().err()
    );
    assert!(native_result.is_ok(), "原生请求必须成功：{native_result:?}");
    assert!(
        upstream_result.is_ok(),
        "上游请求必须成功：{upstream_result:?}"
    );
}
