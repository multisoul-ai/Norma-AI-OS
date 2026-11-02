#![cfg(any(unix, windows))]
use norma::codex::relay::CodexQuestionRelay;
use norma::codex::{UnixListener, UnixStream};
use serde_json::json;
use std::time::Duration;
#[path = "support/codex_relay.rs"]
mod support;
use support::{read, send, timed};

use tungstenite::Message;

/// 场景：任一端短暂停止读取，大帧必须受背压而非撑爆relay缓冲并断开原TUI。
/// 构造：48条各512KiB消息共24MiB，超过本机socket队列容量；接收端暂时不消费。
/// 执行：分别以native/server为来源，连续写后等待1秒确认尚无错误；恢复读取后逐条核对并回复。
/// 预期：内存有界、连接存活，全部帧保持顺序与内容，观察器不得因此拆掉原生会话。
#[test]
fn slow_server_applies_backpressure_without_closing_the_native_connection() {
    for from_native in [true, false] {
        slow_peer(from_native);
    }
}

fn slow_peer(from_native: bool) {
    let dir = tempfile::Builder::new()
        .prefix("nqr-")
        .tempdir_in(if cfg!(windows) {
            std::env::temp_dir()
        } else {
            "/tmp".into()
        })
        .unwrap();
    let path = dir.path().join("server.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let (native, bridge) = UnixStream::pair().unwrap();
    let connector = std::thread::spawn(move || CodexQuestionRelay::connect(bridge, &path).unwrap());
    let server = tungstenite::accept(timed(listener.accept().unwrap().0)).unwrap();
    let native = tungstenite::client("ws://localhost/", timed(native))
        .unwrap()
        .0;
    let relay = connector.join().unwrap();
    let (mut source, mut sink) = if from_native {
        (native, server)
    } else {
        (server, native)
    };
    let (release, gate) = crossbeam_channel::bounded(1);
    let (sent, sending) = crossbeam_channel::bounded(1);
    let (started, first_frame) = crossbeam_channel::bounded(1);
    let server = std::thread::spawn(move || {
        gate.recv_timeout(Duration::from_secs(5)).unwrap();
        for index in 0..48 {
            let frame = read(&mut sink);
            assert_eq!(frame["id"], index, "背压不得丢帧或乱序，方向={from_native}");
            assert_eq!(
                frame["params"]["text"].as_str().unwrap().len(),
                512 * 1024,
                "正文保持完整，方向={from_native}"
            );
        }
        send(&mut sink, json!({"id":"all","result":{}}));
        let _ = sink.read();
    });
    let tui = std::thread::spawn(move || {
        for index in 0..48 {
            let frame = json!({"id":index,"method":"large/passthrough","params":{"text":"x".repeat(512*1024)}});
            if let Err(error) = source.send(Message::Text(frame.to_string().into())) {
                sent.send(Err(error.to_string())).unwrap();
                return;
            }
            if index == 0 {
                started.send(()).unwrap();
            }
        }
        sent.send(Ok(())).unwrap();
        let mut pings = 0;
        let acknowledgement: serde_json::Value = loop {
            match source.read().expect("来源须收到协议确认") {
                Message::Ping(_) => pings += 1,
                Message::Text(text) => break serde_json::from_str(&text).unwrap(),
                _ => {}
            }
        };
        eprintln!("方向={from_native}，真实来源收到Ping={pings}");
        #[cfg(windows)]
        assert!(
            pings > 0,
            "慢端完整性必须实际经过反向Ping，方向={from_native}"
        );
        assert_eq!(
            acknowledgement["id"], "all",
            "TUI收到完整传输的确认，方向={from_native}"
        );
        source.close(None).unwrap();
    });
    first_frame.recv_timeout(Duration::from_secs(5)).unwrap();
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
