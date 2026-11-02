use super::{partial_tests::ControlledStream, CodexQuestionRelay, UnixStream};
use crate::codex::UnixListener;
use serde_json::json;
use std::net::Shutdown;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tungstenite::{Message, WebSocket};

fn timed(stream: UnixStream) -> UnixStream {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
}

/// 场景：relay向暂停读取的server实际连续WouldBlock后，native写端关闭必须结束relay。
/// 构造：最多128×512KiB=64MiB独立帧；真实输出Socket观测8次连续WouldBlock，任何真实写进展都清零计数。
/// 执行：只在relay输出见证成立后关闭native，随后在原3秒关闭预算内等待并回收资源。
/// 预期：不是外部writer瞬时背压的假阳性；前提不成立也必须失败，关闭后不能遗留relay。
#[test]
fn actual_relay_output_backpressure_detects_native_shutdown() {
    let dir = tempfile::Builder::new()
        .prefix("nob-")
        .tempdir_in(if cfg!(windows) {
            std::env::temp_dir()
        } else {
            "/tmp".into()
        })
        .unwrap();
    let path = dir.path().join("s.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let (native, bridge) = UnixStream::pair().unwrap();
    #[cfg(windows)]
    let native_probe = bridge.try_clone().unwrap();
    let blocked = Arc::new(AtomicUsize::new(0));
    let witness = blocked.clone();
    let connector = std::thread::spawn(move || {
        CodexQuestionRelay::connect_with(
            bridge,
            || {
                Ok(ControlledStream::observing(
                    UnixStream::connect(&path)?,
                    witness,
                ))
            },
            false,
            true,
            false,
        )
        .unwrap()
    });
    let server = tungstenite::accept(timed(listener.accept().unwrap().0)).unwrap();
    let mut native: WebSocket<UnixStream> = tungstenite::client("ws://localhost/", timed(native))
        .unwrap()
        .0;
    let relay = connector.join().unwrap();
    let shutdown = native.get_ref().try_clone().unwrap();
    let frames = Arc::new(AtomicUsize::new(0));
    let sent = frames.clone();
    let writer = std::thread::spawn(move || {
        let payload =
            json!({"method":"load/passthrough","params":{"text":"x".repeat(512*1024)}}).to_string();
        for _ in 0..128 {
            if native.send(Message::Text(payload.clone().into())).is_err() {
                return;
            }
            sent.fetch_add(1, Ordering::AcqRel);
        }
    });
    let deadline = Instant::now() + Duration::from_secs(4);
    while blocked.load(Ordering::Acquire) < 8 && relay.is_running() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    let observed = blocked.load(Ordering::Acquire);
    eprintln!(
        "relay output pre-close consecutive WouldBlock={observed}; complete source frames={}",
        frames.load(Ordering::Acquire)
    );
    let was_running = relay.is_running();
    shutdown.shutdown(Shutdown::Both).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while relay.is_running() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let stopped = !relay.is_running();
    #[cfg(windows)]
    readiness_witness(&native_probe);
    #[cfg(windows)]
    eprintln!(
        "relay close source writer finished before final handle drop={}",
        writer.is_finished()
    );
    drop(shutdown);
    #[cfg(windows)]
    if !stopped {
        let after_drop = Instant::now() + Duration::from_millis(300);
        while relay.is_running() && Instant::now() < after_drop {
            std::thread::sleep(Duration::from_millis(5));
        }
        eprintln!(
            "relay close after final source handle drop: running={}",
            relay.is_running()
        );
        readiness_witness(&native_probe);
    }
    drop(server);
    drop(relay);
    writer.join().unwrap();
    assert!(
        was_running,
        "关闭前relay必须仍然存活，不能把背压导致的提前断连冒充关闭成功"
    );
    assert!(
        observed >= 8,
        "必须观测relay实际输出连续背压，不能用外部writer首次WouldBlock代替：{observed}"
    );
    assert!(stopped, "实际relay输出背压时native关闭必须在3秒内收口");
}

#[cfg(windows)]
fn readiness_witness(stream: &UnixStream) {
    use std::os::windows::io::AsRawSocket;
    use windows_sys::Win32::Networking::WinSock::{
        ioctlsocket, WSAGetLastError, WSAPoll, FIONREAD, POLLRDNORM, WSAPOLLFD,
    };
    let mut unread = 0u32;
    let status = unsafe { ioctlsocket(stream.as_raw_socket() as _, FIONREAD, &mut unread) };
    eprintln!("relay close native FIONREAD: status={status}, unread_bytes={unread}");
    for events in [0, POLLRDNORM] {
        let mut descriptor = WSAPOLLFD {
            fd: stream.as_raw_socket() as _,
            events,
            revents: 0,
        };
        let result = unsafe { WSAPoll(&mut descriptor, 1, 0) };
        let error = if result < 0 {
            unsafe { WSAGetLastError() }
        } else {
            0
        };
        eprintln!("relay close native WSAPoll: interest={events}, result={result}, revents={}, error={error}", descriptor.revents);
    }
}
