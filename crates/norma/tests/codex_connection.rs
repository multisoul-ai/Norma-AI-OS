#![cfg(unix)]
use norma::codex::CodexConnection;
use serde_json::{json, Value};
use std::os::unix::net::UnixListener;
use std::time::Duration;
use tungstenite::Message;

/// 私有 Unix WebSocket 正确关联乱序 RPC、服务端审批和完整长消息，断开后不可继续发送。
///
/// 数据构造：2 个同时请求、1 条审批通知、4000 字符正文（大于现有摘要 1200 字符）；超时 5s 防止本地死锁。
/// 执行过程：初始化 → 服务端逆序回复两 RPC 并发出审批 → 客户端精确拒绝请求 7 → 关闭连接。
/// 预期结果：响应不会串线、正文不截断；审批 ID/结果原样回传，断线后不能静默重放命令。
#[test]
fn unix_connection_routes_replies_and_permissions_without_truncation() {
    let directory = tempfile::tempdir().expect("应创建私有目录");
    let socket_path = directory.path().join("codex.sock");
    let listener = UnixListener::bind(&socket_path).expect("应绑定专用 socket");
    let server = std::thread::spawn(move || {
        let (socket, _) = listener.accept().expect("应接入");
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("应设防挂超时");
        let mut ws = tungstenite::accept(socket).expect("WebSocket 握手应成功");
        let init: Value =
            serde_json::from_str(ws.read().expect("初始化").to_text().expect("JSON帧"))
                .expect("初始化JSON");
        assert_eq!(init["method"], "initialize", "必须先初始化协议");
        ws.send(Message::Text(
            json!({"id":init["id"], "result":{"userAgent":"test-codex"}})
                .to_string()
                .into(),
        ))
        .expect("应确认初始化");
        let initialized: Value =
            serde_json::from_str(ws.read().expect("initialized").to_text().expect("JSON"))
                .expect("JSON");
        assert_eq!(initialized["method"], "initialized", "必须完成初始化握手");
        let first: Value =
            serde_json::from_str(ws.read().expect("首请求").to_text().expect("JSON"))
                .expect("JSON");
        let second: Value =
            serde_json::from_str(ws.read().expect("第二请求").to_text().expect("JSON"))
                .expect("JSON");
        ws.send(Message::Text(json!({"id":7,"method":"item/commandExecution/requestApproval","params":{"threadId":"thread-a","turnId":"turn-a","itemId":"item-a"}}).to_string().into())).expect("应发审批");
        ws.send(Message::Text(
            json!({"id":second["id"],"result":{"turn":{"id":"turn-a"}}})
                .to_string()
                .into(),
        ))
        .expect("应先回复第二条");
        ws.send(Message::Text(
            json!({"id":first["id"],"result":{"text":"x".repeat(4000)}})
                .to_string()
                .into(),
        ))
        .expect("应回复完整历史");
        let decision: Value =
            serde_json::from_str(ws.read().expect("应收到审批响应").to_text().expect("JSON"))
                .expect("JSON");
        assert_eq!(decision["id"], 7, "审批不能误用客户端 RPC ID");
        assert_eq!(
            decision["result"]["decision"], "decline",
            "不能扩大为 accept"
        );
        ws.close(None).expect("应关闭连接");
    });
    let client = CodexConnection::connect(&socket_path).expect("应可连接专用 host");
    let history = client
        .request("thread/read", json!({"threadId":"thread-a"}))
        .expect("应可请求历史");
    let turn = client
        .request("turn/start", json!({"threadId":"thread-a"}))
        .expect("应可发起请求");
    let event = client
        .events()
        .recv_timeout(Duration::from_secs(5))
        .expect("应收到服务端审批");
    assert_eq!(event["id"], 7, "事件须保持真实 request ID");
    assert_eq!(
        turn.recv_timeout(Duration::from_secs(5))
            .expect("应收第二响应")
            .expect("RPC成功")["turn"]["id"],
        "turn-a",
        "乱序回复不得串线"
    );
    assert_eq!(
        history
            .recv_timeout(Duration::from_secs(5))
            .expect("应收首响应")
            .expect("RPC成功")["text"]
            .as_str()
            .expect("正文")
            .len(),
        4000,
        "完整历史不能裁为摘要"
    );
    client
        .respond(json!(7), json!({"decision":"decline"}))
        .expect("应答具体审批");
    server.join().expect("服务端断言应通过");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while client.is_connected() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!client.is_connected(), "断开必须可见");
    assert!(
        client.request("turn/start", json!({})).is_err(),
        "断开后不得排队重放写操作"
    );
}

/// 事件消费者停滞时，连接必须显式失败，不能漏掉审批后继续允许控制。
///
/// 数据构造：1 个在途请求和 256 条通知，超过 128 条有界队列的两倍；5s 仅作死锁保护。
/// 执行过程：初始化 → 请求历史但不返回 → 连续发通知且客户端不消费 → 等待错误回执。
/// 预期结果：在途请求失败且连接关闭；之后的批准不能入队，不能得到伪造成功结果。
#[test]
fn event_overflow_fails_pending_requests_and_disables_decisions() {
    let directory = tempfile::tempdir().expect("应创建私有目录");
    let path = directory.path().join("codex.sock");
    let listener = UnixListener::bind(&path).expect("应绑定");
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("应连接");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("设置死锁保护");
        let mut socket = tungstenite::accept(stream).expect("应握手");
        let init: Value =
            serde_json::from_str(socket.read().expect("初始化").to_text().expect("JSON"))
                .expect("初始化JSON");
        socket
            .send(Message::Text(
                json!({"id":init["id"],"result":{}}).to_string().into(),
            ))
            .expect("应回复初始化");
        socket.read().expect("initialized");
        socket.read().expect("应收到历史请求");
        for index in 0..256 {
            if socket.send(Message::Text(json!({"method":"item/agentMessage/delta","params":{"delta":index.to_string()}}).to_string().into())).is_err() { break; }
        }
        // Keep the server open until the client closes; EOF must originate from overflow.
        let _ = socket.read();
    });
    let client = CodexConnection::connect(&path).expect("应连接");
    let reply = client
        .request("thread/read", json!({"threadId":"a"}))
        .expect("应接受请求");
    let error = reply
        .recv_timeout(Duration::from_secs(5))
        .expect("不能永远等待")
        .expect_err("丢事件后请求必须失败");
    assert!(
        error.contains("event gap"),
        "必须明确暴露事件缺口，实际：{error}"
    );
    assert!(!client.is_connected(), "丢事件后不能继续显示在线");
    assert!(
        client
            .respond(json!(7), json!({"decision":"accept"}))
            .is_err(),
        "连接失效时不能提交批准"
    );
    server.join().expect("服务端应退出");
}
