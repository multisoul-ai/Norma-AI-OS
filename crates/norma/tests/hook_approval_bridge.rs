use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};

/// 真实 bridge 进程将单次审批的 HTTP 正文返回 Claude stdout，普通事件不输出授权。
///
/// 数据构造：一个 loopback listener、一个真实 Hook 子进程；拒绝 JSON 分成逐字节写入，模拟 TCP 分包。
/// 超时 = 5 秒，仅用于本机测试防止挂起；生产审批上限另由 APPROVAL_TIMEOUT 控制。
/// 执行过程：输入 PermissionRequest → 验证 POST 与运行实例 → 服务端拒绝 → 等待子进程退出。
/// 预期结果：stdout 为精确拒绝 JSON，原始 session 与 prompt_id 保留；不输出 allow 或网络诊断。
#[test]
fn real_bridge_returns_only_the_bound_permission_response() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("本机监听应可创建");
    let address = listener.local_addr().expect("应有地址");
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("bridge 应发起连接");
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .expect("应设置防挂超时");
        let mut reader = BufReader::new(&socket);
        let mut line = String::new();
        reader.read_line(&mut line).expect("应读取请求行");
        assert!(
            line.starts_with("POST /agent-hooks/approval "),
            "审批必须走独立入口"
        );
        let mut size = 0;
        loop {
            line.clear();
            reader.read_line(&mut line).expect("应读取头");
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.strip_prefix("Content-Length: ") {
                size = value.trim().parse().expect("长度应合法");
            }
        }
        let mut body = vec![0; size];
        reader.read_exact(&mut body).expect("应读取完整请求");
        let request: serde_json::Value = serde_json::from_slice(&body).expect("请求应为 JSON");
        assert_eq!(request["run_id"], "run-a", "必须携带明确运行实例");
        assert_eq!(
            request["event"]["session_id"], "session-a",
            "不能丢失 Provider 会话"
        );
        assert_eq!(
            request["event"]["turn_id"], "turn-a",
            "不能丢失 Claude prompt_id"
        );
        let body = r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"deny","message":"declined"}}}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        for byte in response.bytes() {
            socket.write_all(&[byte]).expect("分包响应应可写入");
        }
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_multisoul-agent-hook"))
        .args(["forward", "--source", "claude", "--request-approval"])
        .env("MULTISOUL_NODE_ID", "7")
        .env("MULTISOUL_HOOK_TOKEN", "test-token")
        .env(
            "MULTISOUL_HOOK_ENDPOINT",
            format!("http://{address}/agent-hooks"),
        )
        .env("NORMA_CONTROL_RUN_ID", "run-a")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("bridge 应可启动");
    child.stdin.take().expect("应有 stdin").write_all(br#"{"hook_event_name":"PermissionRequest","session_id":"session-a","prompt_id":"turn-a","tool_name":"Bash","tool_input":{"command":"pwd"}}"#).expect("应可输入事件");
    let result = child.wait_with_output().expect("bridge 应退出");
    assert!(
        result.status.success(),
        "bridge 失败: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    server.join().expect("服务端检查应通过");
    let output: serde_json::Value =
        serde_json::from_slice(&result.stdout).expect("stdout 只能包含有效 JSON");
    assert_eq!(
        output["hookSpecificOutput"]["decision"]["behavior"], "deny",
        "拒绝必须透传"
    );
    assert_ne!(
        output["hookSpecificOutput"]["decision"]["behavior"], "allow",
        "不得将拒绝变为批准"
    );
}
