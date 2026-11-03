use super::Readiness;
use crate::codex::relay::{partial_tests::ControlledStream, Socket, UnixStream};
use serde_json::json;
use std::{
    io,
    os::windows::io::AsRawSocket,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tungstenite::{
    protocol::{Role, WebSocketConfig},
    Error, Message, WebSocket,
};
use windows_sys::Win32::Networking::WinSock::{ioctlsocket, FIONREAD};

fn available(stream: &UnixStream) -> u32 {
    let mut count = 0;
    assert_eq!(
        unsafe { ioctlsocket(stream.as_raw_socket() as _, FIONREAD, &mut count) },
        0,
        "真实接收队列长度必须可查询"
    );
    count
}

/// 缺少新WinSock API时，超出250ms探测预算也不能越过pending answer的写入所有权。
/// 构造：8KiB正文大于4KiB写预算，64KiB缓冲上限容纳完整帧；真实socket接收队列见证部分写出。
/// 执行：先耗尽4KiB预算，恢复底层可写但不flush，将探测时刻提前1秒并以answer_pending=true探测。
/// 预期：缺API无IOCP资源；探测不增加对端字节；显式flush后精确收到完整原RPC。
#[test]
fn missing_socket_api_probe_does_not_flush_a_pending_answer() {
    let (native, _native_peer) = UnixStream::pair().expect("独立native socket对");
    let (server, peer) = UnixStream::pair().expect("独立server socket对");
    for socket in [&native, &server, &peer] {
        socket.set_nonblocking(true).expect("真实非阻塞socket");
    }
    let budget = Arc::new(AtomicUsize::new(4096));
    let mut native = Socket::from_raw_socket(ControlledStream::from(native), Role::Server, None);
    let config = WebSocketConfig::default()
        .write_buffer_size(0)
        .max_write_buffer_size(64 * 1024);
    let mut server = Socket::from_raw_socket(
        ControlledStream::budgeted(server, budget.clone()),
        Role::Server,
        Some(config),
    );
    let mut peer = WebSocket::from_raw_socket(peer, Role::Client, None);
    let mut readiness =
        Readiness::without_socket_notifications(&native, &server).expect("缺API必须可建立回退");
    let text = json!({"id":7,"result":{"text":"x".repeat(8192)}}).to_string();
    let result = server.send(Message::Text(text.clone().into()));
    assert!(
        matches!(result, Err(Error::Io(ref error)) if error.kind()==io::ErrorKind::WouldBlock),
        "必须真正部分写出后暂停：{result:?}"
    );
    assert_eq!(budget.load(Ordering::Acquire), 0, "必须消耗完整4KiB预算");
    let before = available(peer.get_ref());
    assert_eq!(before, 4096, "真实对端必须持有4KiB未读帧前缀");
    budget.store(usize::MAX, Ordering::Release);
    readiness.last_probe = [Instant::now() - Duration::from_secs(1); 2];
    let (mut native_ready, mut server_ready) = (false, true);
    readiness
        .probe_backpressure(
            &mut native,
            &mut server,
            &mut native_ready,
            &mut server_ready,
            true,
        )
        .expect("pending answer探测不得发送或flush");
    assert_eq!(
        available(peer.get_ref()),
        before,
        "即使底层恢复可写，探测也不能冲出答案或Ping"
    );
    assert!(!native_ready, "探测不能伪造native重新可写");
    assert!(server_ready, "无实际IO时server可写标记保持不变");
    server.flush().expect("显式批准的后续flush应完成");
    assert_eq!(
        peer.read().expect("完整帧只在显式flush后出现"),
        Message::Text(text.into()),
        "不能丢失或改写原RPC"
    );
}
