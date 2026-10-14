mod support;
use norma::connector::*;
use serde_json::json;
use std::{net::Ipv4Addr, sync::Arc};
use support::Credentials;
struct Empty;
impl DataSource for Empty {
    fn snapshot(&self) -> anyhow::Result<Snapshot> {
        Ok(Snapshot::default())
    }
}
async fn host(authority: &str) -> (ConnectorServer, Arc<Auth>, Arc<Credentials>, String) {
    let store = Arc::new(Credentials::default());
    let auth = Arc::new(Auth::load(store.clone(), Arc::new(SystemClock)).expect("鉴权"));
    let token = auth.generate().expect("凭据");
    let tools = ConnectorTools::new(
        auth.clone(),
        CardSigner::new(&auth.signer_key().expect("密钥")),
        Arc::new(Empty),
    );
    let server = ConnectorServer::start(
        ConnectorConfig {
            enabled: true,
            port: 0,
            tunnel_authority: Some(authority.parse().expect("主机")),
        },
        auth.clone(),
        tools,
    )
    .await
    .expect("监听")
    .expect("开启");
    (server, auth, store, token)
}
async fn initialize(
    server: &ConnectorServer,
    token: &str,
    host: &str,
    origin: Option<&str>,
    forwarded: Option<&str>,
) -> u16 {
    let mut request = reqwest::Client::new().post(format!("http://{}/mcp", server.address()))
        .bearer_auth(token).header("host", host).header("accept", "application/json, text/event-stream")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"funnel-test","version":"1"}}}));
    if let Some(origin) = origin {
        request = request.header("origin", origin);
    }
    if let Some(ip) = forwarded {
        request = request
            .header("tailscale-funnel-request", "?1")
            .header("x-forwarded-for", ip);
    }
    let response = request.send().await.expect("初始化");
    let status = response.status().as_u16();
    let _ = response.text().await.expect("消耗响应释放连接");
    status
}
/// 场景：配置ts.net authority，通过真实HTTP分别发送本机/隧道Host和Origin。
/// 预期：允许配置主机；其他Host、跨域Origin和端口变化403；无Origin仍可用。
#[tokio::test]
async fn tunnel_host_and_origin_use_the_configured_authorities() {
    let (server, _, _, token) = host("device.tailnet.ts.net").await;
    for (host, origin, status) in [
        ("device.tailnet.ts.net", None, 200),
        ("DEVICE.TAILNET.TS.NET", None, 200),
        (
            "device.tailnet.ts.net",
            Some("https://device.tailnet.ts.net"),
            200,
        ),
        (
            "device.tailnet.ts.net",
            Some("https://device.tailnet.ts.net:443"),
            200,
        ),
        (
            "device.tailnet.ts.net",
            Some("https://device.tailnet.ts.net:8443"),
            403,
        ),
        ("device.tailnet.ts.net", Some("https://evil.example"), 403),
        ("evil.example", None, 403),
        ("device.tailnet.ts.net.evil.example", None, 403),
        ("device.tailnet.ts.net:8443", None, 403),
    ] {
        assert_eq!(
            initialize(&server, &token, host, origin, None).await,
            status,
            "Host={host}, Origin={origin:?}"
        );
    }
    let local = server.address().to_string();
    assert_eq!(
        initialize(
            &server,
            &token,
            &local,
            Some(&format!("http://{local}")),
            None
        )
        .await,
        200,
        "本机同源允许"
    );
    server.stop().await;
    let (server, _, _, token) = host("device.tailnet.ts.net:8443").await;
    assert_eq!(
        initialize(
            &server,
            &token,
            "device.tailnet.ts.net:8443",
            Some("https://device.tailnet.ts.net:8443"),
            None
        )
        .await,
        200,
        "显式外部端口通过Host和Origin"
    );
    assert_eq!(
        initialize(&server, &token, "device.tailnet.ts.net", None, None).await,
        403,
        "显式端口不能省略"
    );
    assert_eq!(
        initialize(
            &server,
            &token,
            "device.tailnet.ts.net:8443",
            Some("https://device.tailnet.ts.net"),
            None
        )
        .await,
        403,
        "Origin端口必须匹配"
    );
    server.stop().await;
}
/// 场景：本机进程伪造完整Funnel元组；轮换110个XFF后再伪造另一IP。
/// 构造：配置真实隧道Host和?1标记，通过本机TCP请求错误/正确令牌。
/// 预期：全部失败归入127.0.0.1，XFF不能绕过429或锁定其他IP；有效MCP继续工作。
#[tokio::test]
async fn forged_funnel_headers_share_peer_lockout_and_coalesce_audit() {
    let (server, auth, store, token) = host("device.tailnet.ts.net").await;
    let url = format!("http://{}/mcp", server.address());
    let client = reqwest::Client::new();
    for attempt in 1..=110 {
        let response = client
            .post(&url)
            .header("host", "device.tailnet.ts.net")
            .header("tailscale-funnel-request", "?1")
            .header("x-forwarded-for", format!("198.51.100.{attempt}"))
            .bearer_auth("bad")
            .send()
            .await
            .expect("失败请求");
        assert_eq!(
            response.status(),
            if attempt < 10 { 401 } else { 429 },
            "错误令牌达到10次后返回429"
        );
    }
    for last in 1..=110 {
        assert_eq!(
            auth.locked_until_for(format!("198.51.100.{last}").parse().expect("伪造IP")),
            0,
            "不得锁定伪造IP"
        );
    }
    let response = client
        .post(&url)
        .header("host", "device.tailnet.ts.net")
        .header("tailscale-funnel-request", "?1")
        .header("x-forwarded-for", "198.51.100.2")
        .send()
        .await
        .expect("B失败");
    assert_eq!(response.status(), 429, "换XFF不能绕过真实peer锁定");
    assert_eq!(
        auth.locked_until_for("198.51.100.2".parse().expect("IP")),
        0,
        "B未锁定"
    );
    assert!(
        auth.locked_until_for(Ipv4Addr::LOCALHOST.into()) > auth.now(),
        "全部流量归入TCP peer"
    );
    assert_eq!(
        store.audit.lock().expect("审计").len(),
        1,
        "同一peer窗口合并为一条审计，伪造头不分桶"
    );
    assert_eq!(
        initialize(
            &server,
            &token,
            "device.tailnet.ts.net",
            None,
            Some("198.51.100.1")
        )
        .await,
        200,
        "洪水后合法MCP可初始化"
    );
    assert_eq!(
        initialize(
            &server,
            &token,
            "device.tailnet.ts.net",
            None,
            Some("198.51.100.2")
        )
        .await,
        200,
        "其他来源合法令牌同样可用"
    );
    assert_eq!(
        *store
            .auth_failures
            .lock()
            .expect("窗口计数")
            .values()
            .max()
            .expect("A窗口"),
        111,
        "110次洪水加一次缺失令牌，合计111"
    );
    server.stop().await;
}
/// 场景：本机Host伪造Funnel头；隧道Host缺失/伪造标记、IP链、重复头或非法IP。
/// 构造：每类分别发送10个错误令牌；预期全部按TCP peer锁定，伪造来源从未锁定。
#[tokio::test]
async fn direct_and_malformed_forwarding_headers_do_not_change_source() {
    let (server, auth, _, _) = host("device.tailnet.ts.net").await;
    let local = server.address().to_string();
    let url = format!("http://{local}/mcp");
    let client = reqwest::Client::new();
    let mut attempt = 0;
    for (host, marker, forwarded) in [
        (local.as_str(), Some("?1"), "198.51.100.9"),
        ("device.tailnet.ts.net", None, "198.51.100.9"),
        ("device.tailnet.ts.net", Some("true"), "198.51.100.9"),
        (
            "device.tailnet.ts.net",
            Some("?1"),
            "198.51.100.8, 198.51.100.9",
        ),
        ("device.tailnet.ts.net", Some("?1"), "bad"),
    ] {
        for _ in 0..10 {
            attempt += 1;
            let mut request = client
                .post(&url)
                .header("host", host)
                .header("x-forwarded-for", forwarded);
            if let Some(marker) = marker {
                request = request.header("tailscale-funnel-request", marker);
            }
            assert_eq!(
                request.send().await.expect("请求").status(),
                if attempt < 10 { 401 } else { 429 },
                "达到阈值后429"
            );
        }
    }
    assert!(
        auth.locked_until_for(Ipv4Addr::LOCALHOST.into()) > auth.now(),
        "真实peer被锁定"
    );
    assert_eq!(
        auth.locked_until_for("198.51.100.9".parse().expect("伪造IP")),
        0,
        "忽略伪造IP"
    );
    server.stop().await;
}
