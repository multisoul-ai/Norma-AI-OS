use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};

/// 派发身份来自启动环境，不能由 hook JSON 覆盖。
///
/// 数据构造：一个本机 HTTP 监听、node=7、当前 dispatch=current，输入伪造 old。
/// 执行过程：启动真实 norma-hook → 输入 Stop → 接收实际 POST → 检查身份并回收进程。
/// 预期结果：POST 带 current 和原始结果；old 不存在；bridge 正常退出。
#[test]
fn dispatch_identity_comes_from_launch_environment() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listen");
    let address = listener.local_addr().expect("address");
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("connection");
        // 5 秒是仅本机网络读取的防挂上限，不参与业务判定。
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .expect("timeout");
        let mut reader = BufReader::new(&socket);
        let mut size = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("header");
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length: ") {
                size = value.trim().parse().expect("size");
            }
        }
        let mut body = vec![0; size];
        reader.read_exact(&mut body).expect("body");
        let request: serde_json::Value = serde_json::from_slice(&body).expect("json");
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .expect("response");
        request
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_norma-hook"))
        .args(["forward", "--source", "claude"])
        .env("NORMA_NODE_ID", "7")
        .env("NORMA_HOOK_TOKEN", "test-token")
        .env(
            "NORMA_HOOK_ENDPOINT",
            format!("http://{address}/agent-hooks"),
        )
        .env("NORMA_DISPATCH_ID", "current")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("bridge");
    child.stdin.take().expect("stdin").write_all(br#"{"hook_event_name":"Stop","session_id":"s","last_assistant_message":"done","_norma_dispatch_id":"old"}"#).expect("input");
    let result = child.wait_with_output().expect("reap");
    assert!(result.status.success(), "bridge must exit cleanly");
    let request = server.join().expect("server");
    assert_eq!(
        request["raw_json"]["_norma_dispatch_id"], "current",
        "trusted process identity must overwrite payload"
    );
    assert_ne!(
        request["raw_json"]["_norma_dispatch_id"], "old",
        "stale identity must not survive"
    );
    assert_eq!(
        request["raw_json"]["last_assistant_message"], "done",
        "result text must survive injection"
    );
}
