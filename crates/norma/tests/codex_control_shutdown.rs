#![cfg(any(unix, windows))]
use norma::codex::relay::CodexQuestionRelay;
use serde_json::json;
use std::net::Shutdown;
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::{Duration, Instant};
use tungstenite::{Message, WebSocket};
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

/// 场景：任一方向因慢读端施加背压时，写端或读端退出都必须结束原 relay。
/// 构造：真实双向 AF_UNIX WebSocket，48 × 512KiB = 24MiB 超过 socket 缓冲，读端暂停。
/// 执行：预序列化正文，确认真实 send/flush 返回 WouldBlock 后，分别关闭两端并回收线程。
/// 预期：两个方向各两种关闭均在 3s 内结束，发送方收到错误，不能忙等或遗留后台 IO。
#[test]
fn backpressured_relay_closes_when_either_peer_exits_in_both_directions() {
    for from_native in [true, false] {
        for close_writer in [true, false] {
            let dir = tempfile::Builder::new()
                .prefix("ncs-")
                .tempdir_in(if cfg!(windows) {
                    std::env::temp_dir()
                } else {
                    "/tmp".into()
                })
                .unwrap();
            let path = dir.path().join("s.sock");
            let listener = UnixListener::bind(&path).unwrap();
            let (native, bridge) = UnixStream::pair().unwrap();
            let connector =
                std::thread::spawn(move || CodexQuestionRelay::connect(bridge, &path).unwrap());
            let server = tungstenite::accept(timed(listener.accept().unwrap().0)).unwrap();
            let native = tungstenite::client("ws://localhost/", timed(native))
                .unwrap()
                .0;
            let relay = connector.join().unwrap();
            let (mut writer, reader): (WebSocket<UnixStream>, WebSocket<UnixStream>) =
                if from_native {
                    (native, server)
                } else {
                    (server, native)
                };
            let shutdown = writer.get_ref().try_clone().unwrap();
            let mut reader = Some(reader);
            writer.get_mut().set_nonblocking(true).unwrap();
            let payload = json!({"id":"load","method":"load/passthrough","params":{"text":"x".repeat(512*1024)}}).to_string();
            let (blocked, observed) = crossbeam_channel::bounded(1);
            let (done, finished) = crossbeam_channel::bounded(1);
            let worker = std::thread::spawn(move || {
                let result = (|| -> Result<(), String> {
                    for _ in 0..48 {
                        match writer.send(Message::Text(payload.clone().into())) {
                            Ok(()) => {}
                            Err(tungstenite::Error::Io(error))
                                if error.kind() == std::io::ErrorKind::WouldBlock =>
                            {
                                let _ = blocked.try_send(());
                            }
                            Err(error) => return Err(error.to_string()),
                        }
                        loop {
                            match writer.flush() {
                                Ok(()) => break,
                                Err(tungstenite::Error::Io(error))
                                    if error.kind() == std::io::ErrorKind::WouldBlock =>
                                {
                                    let _ = blocked.try_send(());
                                    std::thread::sleep(Duration::from_millis(1));
                                }
                                Err(error) => return Err(error.to_string()),
                            }
                        }
                    }
                    Ok(())
                })();
                done.send(result).unwrap();
            });
            let backpressured = observed.recv_timeout(Duration::from_secs(5)).is_ok();
            let early = finished.try_recv().ok();
            if close_writer {
                shutdown.shutdown(Shutdown::Both).unwrap();
            } else {
                drop(reader.take());
            }
            let deadline = Instant::now() + Duration::from_secs(3);
            while relay.is_running() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
            let stopped = !relay.is_running();
            // 先收口所有测试资源，再断言，失败路径也不能留下阻塞线程。
            drop(reader);
            drop(relay);
            let _ = shutdown.shutdown(Shutdown::Both);
            worker.join().unwrap();
            let result =
                early.unwrap_or_else(|| finished.recv_timeout(Duration::from_secs(5)).unwrap());
            assert!(
                backpressured,
                "必须实测到背压，方向={from_native}，关闭写端={close_writer}，提前结果={result:?}"
            );
            assert!(
                stopped,
                "背压时也必须检测任一端关闭，方向={from_native}，关闭写端={close_writer}"
            );
            assert!(
                result.is_err(),
                "关闭不能伪报全部帧发送成功，方向={from_native}，关闭写端={close_writer}"
            );
        }
    }
}
