#![cfg(windows)]
use std::{
    io::{Read, Write},
    net::Shutdown,
    os::windows::io::AsRawSocket,
};
use uds_windows::UnixStream;
use windows_sys::Win32::Networking::WinSock::{
    ioctlsocket, send, WSAGetLastError, WSAIoctl, WSAPoll, WSASend, FIONREAD, POLLRDNORM,
    POLLWRNORM, SIO_AF_UNIX_GETPEERPID, WSABUF, WSAPOLLFD,
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
    let mut empty = 0u8;
    let buffer = WSABUF {
        len: 0,
        buf: &mut empty,
    };
    let mut sent = 0;
    let result = unsafe { WSASend(fd, &buffer, 1, &mut sent, 0, std::ptr::null_mut(), None) };
    eprintln!(
        "{label}: WSASend(0)={result}, sent={sent}, error={}",
        unsafe { WSAGetLastError() }
    );
    let mut pid = 0u32;
    let mut returned = 0;
    let result = unsafe {
        WSAIoctl(
            fd,
            SIO_AF_UNIX_GETPEERPID,
            std::ptr::null(),
            0,
            (&mut pid as *mut u32).cast(),
            size_of::<u32>() as u32,
            &mut returned,
            std::ptr::null_mut(),
            None,
        )
    };
    // The PID is never used as a liveness test; only report the socket ioctl status.
    eprintln!(
        "{label}: peer-state ioctl={result}, bytes={returned}, error={}",
        unsafe { WSAGetLastError() }
    );
}

/// 场景：将复制句柄与发送队列积压分开，定位未读正文遮蔽关闭通知的最小条件。
/// 构造：三个独立开关控制source clone、receiver clone和4MiB发送负载；正文固定为7。
/// 执行：只读端不消费，写到WouldBlock或完整128KiB；shutdown并释放全部source，再记录探测。
/// 预期：存活探测不破坏协议，关闭后逐字节排空全部已接受正文；不得以PID活性判定关闭。
#[test]
fn socket_close_with_clones_and_saturated_sender_preserves_payload() {
    for source_clone in [false, true] {
        for receiver_clone in [false, true] {
            for saturate in [false, true] {
                let label = format!("source_clone={source_clone}/receiver_clone={receiver_clone}/saturate={saturate}");
                let (mut source, mut receiver) = UnixStream::pair().unwrap();
                let source_copy = source_clone.then(|| source.try_clone().unwrap());
                let receiver_copy = receiver_clone.then(|| receiver.try_clone().unwrap());
                source.set_nonblocking(true).unwrap();
                receiver.set_nonblocking(true).unwrap();
                let bytes = vec![
                    7u8;
                    if saturate {
                        4 * 1024 * 1024
                    } else {
                        128 * 1024
                    }
                ];
                let mut written = 0;
                let mut blocked = false;
                while written < bytes.len() {
                    match source.write(&bytes[written..]) {
                        Ok(0) => panic!("真实socket不能零长接受非空正文"),
                        Ok(count) => written += count,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            blocked = true;
                            break;
                        }
                        Err(error) => panic!("{label}: {error}"),
                    }
                }
                assert!(written > 0, "{label}: 必须实际发送正文");
                eprintln!("{label}: accepted={written}, blocked={blocked}");
                observe(&format!("{label}/alive"), &receiver);
                source_copy
                    .as_ref()
                    .unwrap_or(&source)
                    .shutdown(Shutdown::Both)
                    .unwrap();
                drop(source_copy);
                drop(source);
                observe(&format!("{label}/closed"), &receiver);
                receiver.set_nonblocking(false).unwrap();
                receiver
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                let mut actual = Vec::new();
                receiver
                    .read_to_end(&mut actual)
                    .expect("关闭后应排空真实正文并抵达EOF");
                assert_eq!(
                    actual,
                    bytes[..written],
                    "{label}: 非消费探测必须保持全部已发送正文"
                );
                drop(receiver_copy);
            }
        }
    }
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
