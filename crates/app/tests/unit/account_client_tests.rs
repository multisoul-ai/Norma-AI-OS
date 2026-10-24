use super::client::{account_base_url, CloudAuthApi, CLOUD_URL_ENV};
use account::{AuthApi, AuthError, Provider, TokenRequest};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

/// 单次请求的假 norma-cloud：记录原始请求并返回给定状态与 JSON。
fn serve_once(status: &str, body: String) -> (String, JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("应能绑定假服务端口");
    let base = format!(
        "http://127.0.0.1:{}",
        listener.local_addr().expect("应有本地地址").port()
    );
    let status = status.to_string();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("应收到一个连接");
        // 请求头与请求体可能分多次到达（含分块编码）：读到静默 300ms 为止。
        stream
            .set_read_timeout(Some(std::time::Duration::from_millis(300)))
            .expect("应能设置读超时");
        let mut buffer = vec![0u8; 8192];
        let mut raw = String::new();
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => raw.push_str(&String::from_utf8_lossy(&buffer[..n])),
                Err(_) => break,
            }
        }
        write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).expect("应能写响应");
        raw
    });
    (base, handle)
}

fn token_response() -> String {
    serde_json::json!({
        "schema_version": 1, "access_token": "a1", "refresh_token": "r1", "expires_in": 3600,
        "profile": {"account_id": "acc_1", "display_name": "Zhou Kai", "email": null, "avatar_url": null,
            "identities": [{"provider": "github", "subject": "42"}]}
    })
    .to_string()
}

/// 令牌交换：请求路径、JSON 体与会话映射。
///
/// 数据构造：假服务返回 expires_in = 3600 的会话。
/// 预期结果：
///   - 正断言：请求为 POST /v1/auth/token，体含 code 与 code_verifier；会话令牌与身份被映射；expires_at ≈ now + 3600（±5 秒）
///   - 负断言：请求头中没有 Authorization（交换前尚无令牌）
#[test]
fn test_exchange_posts_code_and_maps_session() {
    let (base, server) = serve_once("200 OK", token_response());
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("时间正常")
        .as_secs();
    let session = CloudAuthApi::new(base)
        .exchange(&TokenRequest {
            code: "c-1".into(),
            code_verifier: "v-1".into(),
        })
        .expect("交换应成功");
    let raw = server.join().expect("假服务线程应正常结束");
    assert!(
        raw.starts_with("POST /v1/auth/token "),
        "应 POST 到 /v1/auth/token，实际：{raw}"
    );
    let body = raw
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or_default();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(body).ok(),
        Some(serde_json::json!({"code": "c-1", "code_verifier": "v-1"})),
        "请求体应是含 code 与 code_verifier 的 JSON，实际请求：{raw:?}"
    );
    assert!(
        !raw.to_ascii_lowercase().contains("authorization:"),
        "交换请求不应带 Authorization"
    );
    assert_eq!(session.access_token, "a1", "应映射 access token");
    assert_eq!(
        session.profile.primary_provider(),
        Some(Provider::GitHub),
        "应映射登录来源"
    );
    assert!(
        session.expires_at >= before + 3600 && session.expires_at <= before + 3600 + 5,
        "expires_at 应约等于 now + 3600，实际 {}",
        session.expires_at
    );
}

/// 错误映射：400 → Rejected(错误码)，401 → Unauthorized，连接失败 → Network。
///
/// 预期结果：三种失败各自映射到对应的 AuthError，且不把整个响应体或 URL 泄露进错误。
#[test]
fn test_error_mapping() {
    let (base, server) = serve_once("400 Bad Request", "{\"error\":\"invalid_grant\"}".into());
    let result = CloudAuthApi::new(base).exchange(&TokenRequest {
        code: "c".into(),
        code_verifier: "v".into(),
    });
    server.join().expect("假服务线程应正常结束");
    assert_eq!(
        result,
        Err(AuthError::Rejected("invalid_grant".into())),
        "400 应映射为带错误码的 Rejected"
    );

    let (base, server) = serve_once("401 Unauthorized", "{\"error\":\"invalid_token\"}".into());
    let result = CloudAuthApi::new(base).me("tok");
    let raw = server.join().expect("假服务线程应正常结束");
    assert_eq!(
        result.map(|_| ()),
        Err(AuthError::Unauthorized),
        "401 应映射为 Unauthorized"
    );
    assert!(
        raw.contains("Bearer tok") || raw.contains("bearer tok"),
        "me 应带 Bearer 令牌"
    );

    let closed = TcpListener::bind("127.0.0.1:0").expect("应能绑定端口");
    let port = closed.local_addr().expect("应有本地地址").port();
    drop(closed);
    let result = CloudAuthApi::new(format!("http://127.0.0.1:{port}")).logout("tok");
    assert!(
        matches!(result, Err(AuthError::Network(_))),
        "连接被拒绝应映射为 Network，实际：{result:?}"
    );
}

/// 默认云端地址与环境变量覆盖。
///
/// 预期结果：正断言：覆盖值去掉末尾斜杠；负断言：空值回落默认（https://soulvoiceai.com/norma），不指向 localhost。
#[test]
fn test_base_url_default_and_override() {
    let previous = std::env::var_os(CLOUD_URL_ENV);
    std::env::set_var(CLOUD_URL_ENV, " http://127.0.0.1:9/ ");
    assert_eq!(
        account_base_url(),
        "http://127.0.0.1:9",
        "覆盖值应去空白与末尾斜杠"
    );
    std::env::set_var(CLOUD_URL_ENV, "  ");
    assert_eq!(
        account_base_url(),
        "https://soulvoiceai.com/norma",
        "空值应回落默认生产地址"
    );
    match previous {
        Some(value) => std::env::set_var(CLOUD_URL_ENV, value),
        None => std::env::remove_var(CLOUD_URL_ENV),
    }
}
