#![cfg(windows)]
use std::{
    io::{Read, Write},
    net::Shutdown,
    os::windows::io::AsRawSocket,
};
use uds_windows::UnixStream;
use windows_sys::Win32::Networking::WinSock::{
    ioctlsocket, send, WSAGetLastError, WSAPoll, FIONREAD, POLLRDNORM, POLLWRNORM, WSAPOLLFD,
};

fn observe(label: &str, socket: &UnixStream) {
    let fd = socket.as_raw_socket() as _;
    let mut available = 0u32;
    let result = unsafe { ioctlsocket(fd, FIONREAD, &mut available) };
    eprintln!("{label}: FIONREAD={result}, bytes={available}");
    for interest in [0, POLLRDNORM, POLLWRNORM] {
        let mut poll = WSAPOLLFD {
            fd,
            events: interest,
            revents: 0,
        };
        let result = unsafe { WSAPoll(&mut poll, 1, 5) };
        eprintln!(
            "{label}: poll={result}, interest={interest}, events={}, error={}",
            poll.revents,
            unsafe { WSAGetLastError() }
        );
    }
    let result = unsafe { send(fd, b"".as_ptr(), 0, 0) };
    eprintln!("{label}: empty send={result}, error={}", unsafe {
        WSAGetLastError()
    });
}

/// 场景：Windows AF_UNIX 未读数据不能遮蔽关闭探测，也不能被探测消耗。
/// 构造：128KiB固定正文，依次记录存活、shutdown和最后句柄释放三态。
/// 执行：只使用非消费型WinSock探测，再通过真实read_to_end校验全部字节。
/// 预期：完整正文不变；日志明确给出各API在未读关闭时的真实结果。
#[test]
fn unread_socket_close_notification_diagnostics_preserve_payload() {
    let (mut source, mut receiver) = UnixStream::pair().unwrap();
    let bytes = vec![7u8; 128 * 1024];
    source.set_nonblocking(true).unwrap();
    let mut written = 0;
    while written < bytes.len() {
        match source.write(&bytes[written..]) {
            Ok(0) => break,
            Ok(count) => written += count,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) => panic!("诊断夹具写入失败：{error}"),
        }
    }
    assert!(written > 0, "诊断必须保留真实未读正文");
    let bytes = bytes[..written].to_vec();
    receiver.set_nonblocking(true).unwrap();
    observe("alive", &receiver);
    source.shutdown(Shutdown::Both).unwrap();
    observe("shutdown", &receiver);
    drop(source);
    observe("closed", &receiver);
    receiver.set_nonblocking(false).unwrap();
    let mut actual = Vec::new();
    receiver.read_to_end(&mut actual).unwrap();
    assert_eq!(actual, bytes, "任何探测均不得消费、改写或丢弃原协议正文");
}
