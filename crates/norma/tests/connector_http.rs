mod support;
use norma::connector::*;
use serde_json::{json, Value};
use std::sync::Arc;
use support::Credentials;
struct Empty;
impl DataSource for Empty {
    fn snapshot(&self) -> anyhow::Result<Snapshot> {
        Ok(Snapshot::default())
    }
}
async fn host() -> (ConnectorServer, Arc<Auth>, Arc<Credentials>, String) {
    let store = Arc::new(Credentials::default());
    let auth = Arc::new(Auth::load(store.clone(), Arc::new(SystemClock)).expect("鉴权"));
    let token = auth.generate().expect("生成凭据");
    let tools = ConnectorTools::new(
        auth.clone(),
        CardSigner::new(&auth.signer_key().expect("身份密钥")),
        Arc::new(Empty),
    );
    let server = ConnectorServer::start(
        ConnectorConfig {
            enabled: true,
            port: 0,
            tunnel_authority: None,
        },
        auth.clone(),
        tools,
    )
    .await
    .expect("监听")
    .expect("启用");
    (server, auth, store, token)
}
fn parsed(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|_| {
        body.lines()
            .filter_map(|l| l.strip_prefix("data: "))
            .find_map(|v| serde_json::from_str(v).ok())
            .expect("SSE JSON-RPC response")
    })
}
/// 场景：关闭默认服务后开启隔离端口，执行缺失/错误令牌、Host 和无关路径请求。
/// 预期：关闭无监听、仅 IPv4 回环监听、失败 401 且同IP窗口合并审计，不发 CORS 头。
#[tokio::test]
async fn off_loopback_auth_host_and_body_limit() {
    let (server, auth, store, token) = host().await;
    assert_eq!(server.address().ip().to_string(), "127.0.0.1", "只绑回环");
    assert_eq!(ConnectorConfig::default().port, 47615, "默认固定端口");
    let tools = ConnectorTools::new(
        auth.clone(),
        CardSigner::new(&auth.signer_key().expect("密钥")),
        Arc::new(Empty),
    );
    assert!(
        ConnectorServer::start(ConnectorConfig::default(), auth.clone(), tools)
            .await
            .expect("默认关闭")
            .is_none(),
        "默认不监听"
    );
    let client = reqwest::Client::new();
    let url = format!("http://{}/mcp", server.address());
    for wrong in [None, Some("bad")] {
        let mut request = client.post(&url).json(&json!({}));
        if let Some(token) = wrong {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.expect("HTTP");
        assert_eq!(response.status(), 401, "缺失/错误凭据返回401");
        assert!(
            response
                .headers()
                .get("access-control-allow-origin")
                .is_none(),
            "不开放CORS"
        );
    }
    assert_eq!(
        store.audit.lock().expect("审计").len(),
        1,
        "两次鉴权失败合并为同IP窗口审计"
    );
    let response = client
        .post(&url)
        .bearer_auth(&token)
        .header("host", "evil.example")
        .json(&json!({}))
        .send()
        .await
        .expect("Host");
    assert_eq!(response.status(), 403, "拒绝非白名单Host");
    let response = client
        .post(&url)
        .bearer_auth(&token)
        .body(vec![b'x'; 65537])
        .send()
        .await
        .expect("body");
    assert_eq!(response.status(), 413, "64KiB+1必须拒绝");
    let response = client
        .get(format!("http://{}/other", server.address()))
        .send()
        .await
        .expect("其他路径");
    assert_eq!(response.status(), 404, "只有/mcp可用");
    let address = server.address();
    server.stop().await;
    assert!(
        tokio::net::TcpStream::connect(address).await.is_err(),
        "关闭后不监听"
    );
}
/// 场景：真实 HTTP initialize、initialized、tools/list；建立 SSE 后吊销。
/// 预期：会话头存在、工具保留三个只读工具并开放四个写工具，吊销断开已有流且旧凭据立即401。
#[tokio::test]
async fn mcp_lists_p4_tools_and_revoke_closes_sessions() {
    let (server, auth, _, token) = host().await;
    let client = reqwest::Client::new();
    let url = format!("http://{}/mcp", server.address());
    let response=client.post(&url).bearer_auth(&token).header("accept","application/json, text/event-stream")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})).send().await.expect("initialize");
    assert_eq!(response.status(), 200, "初始化成功");
    let session = response
        .headers()
        .get("mcp-session-id")
        .expect("会话头")
        .to_str()
        .expect("ASCII")
        .to_owned();
    let init = parsed(&response.text().await.expect("初始化响应"));
    assert!(init.get("result").is_some(), "返回初始化结果");
    let response = client
        .post(&url)
        .bearer_auth(&token)
        .header("accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session)
        .json(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
        .send()
        .await
        .expect("initialized");
    assert_eq!(response.status(), 202, "通知被接受");
    let response = client
        .post(&url)
        .bearer_auth(&token)
        .header("accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session)
        .json(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
        .send()
        .await
        .expect("tools/list");
    let body = parsed(&response.text().await.expect("工具响应"));
    let names: Vec<_> = body["result"]["tools"]
        .as_array()
        .expect("工具列表")
        .iter()
        .map(|t| t["name"].as_str().expect("名称"))
        .collect();
    assert_eq!(
        names,
        vec![
            "directory",
            "list_tasks",
            "get_task_progress",
            "dispatch_task",
            "cancel_task",
            "reply_agent",
            "create_card"
        ],
        "P4保留P2只读工具，追加四个写工具"
    );
    let response=client.post(&url).bearer_auth(&token).header("accept","application/json, text/event-stream").header("mcp-session-id",&session)
        .json(&json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"directory","arguments":{}}})).send().await.expect("真实工具调用");
    let value = parsed(&response.text().await.expect("目录结果"));
    assert_eq!(
        value["result"]["isError"], false,
        "上下文包含此HTTP请求鉴权"
    );
    assert_eq!(
        value["result"]["structuredContent"]["workspaces"],
        json!([]),
        "真实工具输出结构"
    );
    let text = value["result"]["content"][0]["text"]
        .as_str()
        .expect("文本块");
    assert_eq!(
        serde_json::from_str::<Value>(text).expect("文本为JSON"),
        value["result"]["structuredContent"],
        "content[0].text 与 structuredContent 相同"
    );
    let mut stream = client
        .get(&url)
        .bearer_auth(&token)
        .header("accept", "text/event-stream")
        .header("mcp-session-id", &session)
        .send()
        .await
        .expect("SSE");
    assert_eq!(stream.status(), 200, "建立会话流");
    auth.revoke().expect("吊销");
    let closed = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while let Ok(Some(_)) = stream.chunk().await {}
    })
    .await;
    assert!(closed.is_ok(), "吊销必须断开现有SSE流");
    let response = client
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .expect("旧凭据");
    assert_eq!(response.status(), 401, "吊销立即401");
    let regenerated = auth.generate().expect("吊销后重新生成");
    let response=client.post(&url).bearer_auth(&regenerated).header("accept","application/json, text/event-stream")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})).send().await.expect("新授权");
    assert_eq!(response.status(), 200, "重新生成后服务恢复，旧会话不复活");
    let _ = response.text().await.expect("初始化响应");
    server.stop().await;
}

/// 场景：8个客户端只发送不完整请求头，占满连接预算；第9个发送完整请求。
/// 预期：第9个暂未被处理，释放1个连接后才返回401，证明预算限制的是连接而非工具请求。
#[tokio::test]
async fn eight_connection_budget_includes_slow_headers() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (server, _, _, _) = host().await;
    let mut held = Vec::new();
    for _ in 0..8 {
        let mut socket = tokio::net::TcpStream::connect(server.address())
            .await
            .expect("连接");
        socket
            .write_all(b"GET /mcp HTTP/1.1\r\n")
            .await
            .expect("部分头");
        held.push(socket);
    }
    let mut ninth = tokio::net::TcpStream::connect(server.address())
        .await
        .expect("第9个连接进入backlog");
    ninth
        .write_all(
            format!(
                "GET /mcp HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
                server.address()
            )
            .as_bytes(),
        )
        .await
        .expect("完整头");
    let mut buffer = [0; 512];
    let pending = tokio::time::timeout(
        std::time::Duration::from_millis(200),
        ninth.read(&mut buffer),
    )
    .await;
    assert!(pending.is_err(), "第9个连接不能在8个活动连接外被处理");
    drop(held.pop());
    let count = tokio::time::timeout(std::time::Duration::from_secs(2), ninth.read(&mut buffer))
        .await
        .expect("释放后响应")
        .expect("读取");
    assert!(
        String::from_utf8_lossy(&buffer[..count]).contains("401"),
        "释放1个位置后处理第9个请求"
    );
    drop(held);
    server.stop().await;
}

/// 场景：合法令牌客户端声明10字节body，仅发1字节；读取超时=10秒。
/// 预期：12秒内得到408，而不是一直占据连接；等待上限留出2秒调度余量。
#[tokio::test]
async fn slow_body_times_out_after_ten_seconds() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (server, _, _, token) = host().await;
    let mut socket = tokio::net::TcpStream::connect(server.address())
        .await
        .expect("连接");
    socket.write_all(format!("POST /mcp HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {token}\r\nContent-Length: 10\r\nConnection: close\r\n\r\nx",server.address()).as_bytes()).await.expect("慢body");
    let started = std::time::Instant::now();
    let mut body = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(12),
        socket.read_to_string(&mut body),
    )
    .await
    .expect("超时有限")
    .expect("响应");
    assert!(body.contains("408"), "body读超时返回408");
    assert!(
        started.elapsed() >= std::time::Duration::from_secs(10),
        "配置的body读超时为10秒"
    );
    server.stop().await;
}

/// 场景：停止服务器并换端口重开；令牌保持有效，再对被占用端口启动。
/// 预期：明确监听配置的新端口，旧端口关闭；冲突返回错误，不随机换端口。
#[tokio::test]
async fn port_change_restarts_without_revoking_credentials() {
    let (server, auth, _, token) = host().await;
    let old_address = server.address();
    server.stop().await;
    let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("选择隔离端口");
    let port = probe.local_addr().expect("端口").port();
    drop(probe);
    let tools = ConnectorTools::new(
        auth.clone(),
        CardSigner::new(&auth.signer_key().expect("密钥")),
        Arc::new(Empty),
    );
    let server = ConnectorServer::start(
        ConnectorConfig {
            enabled: true,
            port,
            tunnel_authority: None,
        },
        auth.clone(),
        tools.clone(),
    )
    .await
    .expect("新端口")
    .expect("启用");
    assert_eq!(server.address().port(), port, "只用配置端口");
    assert!(
        tokio::net::TcpStream::connect(old_address).await.is_err(),
        "旧端口停止监听"
    );
    let response=reqwest::Client::new().post(format!("http://{}/mcp",server.address())).bearer_auth(&token).header("accept","application/json, text/event-stream")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})).send().await.expect("重开请求");
    assert_eq!(response.status(), 200, "换端口后仍能初始化会话");
    assert!(auth.authenticate(Some(&token)).is_ok(), "换端口不吊销凭据");
    let _ = response.text().await.expect("响应");
    assert!(
        ConnectorServer::start(
            ConnectorConfig {
                enabled: true,
                port,
                tunnel_authority: None,
            },
            auth.clone(),
            tools
        )
        .await
        .is_err(),
        "端口冲突明确失败"
    );
    server.stop().await;
}

/// 场景：本机同IP错误令牌达到10次阈值，锁定15×60=900秒；有效令牌仍可初始化。
/// 构造：前9次错误后再发送第10/11次；执行真实HTTP并检查状态、JSON与Retry-After。
/// 预期：未锁定401无重试头，锁定429携带剩余秒数，正确令牌不被429阻断。
#[tokio::test]
async fn auth_lockout_returns_429_with_retry_metadata() {
    let (server, _, _, token) = host().await;
    let client = reqwest::Client::new();
    let url = format!("http://{}/mcp", server.address());
    for attempt in 1..=11 {
        let response = client
            .post(&url)
            .bearer_auth("bad")
            .send()
            .await
            .expect("错误请求");
        if attempt < 10 {
            assert_eq!(response.status(), 401, "第{attempt}次尚未锁定");
            assert!(
                response.headers().get("retry-after").is_none(),
                "普通鉴权失败不含重试头"
            );
            let body: Value = response.json().await.expect("错误体");
            assert_eq!(body["error"]["code"], "unauthorized", "普通错误令牌仍401");
        } else {
            assert_eq!(response.status(), 429, "第{attempt}次已达到锁定阈值");
            let retry: i64 = response
                .headers()
                .get("retry-after")
                .expect("重试头")
                .to_str()
                .expect("秒数")
                .parse()
                .expect("整数");
            let body: Value = response.json().await.expect("锁定错误体");
            assert_eq!(body["error"]["code"], "rate_limited", "锁定错误码");
            assert_eq!(
                body["error"]["retry_after_s"], retry,
                "正文与头使用同一剩余秒数"
            );
            assert!((1..=900).contains(&retry), "剩余时间在15分钟内");
            assert_eq!(body["error"]["retryable"], true, "锁定可在期限后重试");
        }
    }
    let response = client.post(&url).bearer_auth(&token).header("accept", "application/json, text/event-stream")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"lockout-test","version":"1"}}}))
        .send().await.expect("正确凭据");
    assert_eq!(response.status(), 200, "锁定同IP仍接受正确令牌");
    let _ = response.text().await.expect("释放连接");
    server.stop().await;
}

/// 场景：App拥有的服务退出后，写工具调用应是连接失败而非空目录或成功。
/// 构造：随机Token、临时回环端口；执行stop后用全新HTTP客户端POST派工。
/// 预期：无监听、传输连接错误；Grok Bot「Norma未运行」文案仍需真实客户端走查。
#[tokio::test]
async fn stopped_connector_writes_fail_at_transport() {
    let (server, _, _, token) = host().await;
    let url = format!("http://{}/mcp", server.address());
    server.stop().await;
    let result = reqwest::Client::new().post(url).bearer_auth(token).json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"dispatch_task","arguments":{}}})).send().await;
    assert!(result.is_err(), "退出后不能返回成功或空目录");
}
