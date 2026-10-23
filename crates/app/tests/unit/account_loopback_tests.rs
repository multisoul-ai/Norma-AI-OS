use super::loopback::{LoopbackError, LoopbackListener};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

fn request(port: u16, target: &str, host: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("应能连上回环监听");
    write!(stream, "GET {target} HTTP/1.1\r\nHost: {host}\r\n\r\n").expect("写请求应成功");
    let mut body = String::new();
    stream.read_to_string(&mut body).expect("应能读完响应");
    body
}

/// 收到合法回调：返回请求目标，并向浏览器返回 200 的完成页。
///
/// 数据构造：监听随机端口；客户端发送 `GET /callback?code=a&state=b`，Host = 127.0.0.1:<port>。
/// 预期结果：
///   - 正断言：wait 返回 "/callback?code=a&state=b"；浏览器收到 200 与「可以回到 Norma」
///   - 负断言：端口不为 0（确为系统分配的随机端口）
#[test]
fn test_valid_callback_returns_target_and_serves_done_page() {
    let listener = LoopbackListener::bind().expect("应能绑定回环端口");
    let port = listener.port();
    assert_ne!(port, 0, "应由系统分配随机端口");
    let client = std::thread::spawn(move || {
        request(
            port,
            "/callback?code=a&state=b",
            &format!("127.0.0.1:{port}"),
        )
    });
    let target = listener
        .wait_for_callback(Duration::from_secs(5), &Arc::new(AtomicBool::new(false)))
        .expect("应收到回调");
    let page = client.join().expect("客户端线程应正常结束");
    assert_eq!(target, "/callback?code=a&state=b", "应返回请求目标");
    assert!(
        page.starts_with("HTTP/1.1 200"),
        "浏览器应收到 200，实际：{page}"
    );
    assert!(page.contains("可以回到 Norma"), "应显示完成页");
}

/// 其它路径与非本机 Host 的请求被忽略，随后真正的回调仍能被接收。
///
/// 数据构造：先发 /favicon.ico（404），再发 Host = evil.test 的 /callback（404，防 DNS 重绑定），最后发合法回调。
/// 预期结果：
///   - 负断言：前两个请求得到 404，且不会被当作回调返回
///   - 正断言：最终返回合法回调的目标
#[test]
fn test_ignores_unrelated_and_foreign_host_requests() {
    let listener = LoopbackListener::bind().expect("应能绑定回环端口");
    let port = listener.port();
    let client = std::thread::spawn(move || {
        let favicon = request(port, "/favicon.ico", &format!("127.0.0.1:{port}"));
        let foreign = request(port, "/callback?code=evil&state=x", "evil.test");
        request(
            port,
            "/callback?code=ok&state=s",
            &format!("127.0.0.1:{port}"),
        );
        (favicon, foreign)
    });
    let target = listener
        .wait_for_callback(Duration::from_secs(5), &Arc::new(AtomicBool::new(false)))
        .expect("应收到合法回调");
    let (favicon, foreign) = client.join().expect("客户端线程应正常结束");
    assert!(favicon.starts_with("HTTP/1.1 404"), "无关路径应 404");
    assert!(foreign.starts_with("HTTP/1.1 404"), "非本机 Host 应 404");
    assert_eq!(
        target, "/callback?code=ok&state=s",
        "只应返回合法回调，而不是 evil 请求"
    );
}

/// 超时与取消。
///
/// 数值推导：超时 150ms，轮询间隔 40ms，应在约 150–300ms 内返回；取消标志预先置位则立即返回。
/// 预期结果：正断言：Timeout 与 Cancelled 各自出现；负断言：两者都不返回目标。
#[test]
fn test_timeout_and_cancel() {
    let listener = LoopbackListener::bind().expect("应能绑定回环端口");
    let result = listener.wait_for_callback(
        Duration::from_millis(150),
        &Arc::new(AtomicBool::new(false)),
    );
    assert_eq!(result, Err(LoopbackError::Timeout), "无请求时应超时");

    let cancel = Arc::new(AtomicBool::new(false));
    cancel.store(true, Ordering::SeqCst);
    let listener = LoopbackListener::bind().expect("应能绑定回环端口");
    let result = listener.wait_for_callback(Duration::from_secs(30), &cancel);
    assert_eq!(
        result,
        Err(LoopbackError::Cancelled),
        "取消标志置位应立即返回"
    );
}
