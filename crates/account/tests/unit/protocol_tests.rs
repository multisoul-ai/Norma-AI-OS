use super::fixtures::profile;
use super::*;

/// 授权开始地址的精确格式与编码。
///
/// 数据构造：base 末尾带 "/"，provider = GitHub，challenge = "ch-1_x"，
/// state = "st~a b"（含空格），port = 53682。
/// 预期结果：
///   - 正断言：地址为 base + /v1/auth/github/start，参数按序编码，空格编为 %20
///   - 负断言：不出现 "//v1"（base 末尾斜杠已裁剪）
#[test]
fn test_start_url_format_and_encoding() {
    let request = AuthorizeRequest {
        provider: Provider::GitHub,
        code_challenge: "ch-1_x".to_string(),
        state: "st~a b".to_string(),
    };
    let url = start_url("https://example.test/norma/", &request, 53682);
    assert_eq!(
        url,
        "https://example.test/norma/v1/auth/github/start?code_challenge=ch-1_x&state=st~a%20b&redirect_port=53682",
        "开始地址应带 /v1 前缀并正确编码参数"
    );
    assert!(!url.contains("//v1"), "不得出现重复斜杠");
}

/// 令牌交换请求的线上格式。
///
/// 预期结果：正断言：JSON 恰含 code 与 code_verifier 两个字段。
#[test]
fn test_token_request_wire_format() {
    let json = serde_json::to_value(TokenRequest {
        code: "c".to_string(),
        code_verifier: "v".to_string(),
    })
    .expect("序列化不应失败");
    assert_eq!(
        json,
        serde_json::json!({"code": "c", "code_verifier": "v"}),
        "字段名必须与服务端约定一致"
    );
}

/// 令牌响应转为会话：到期时间 = now + expires_in。
///
/// 数值推导：now = 1_000，expires_in = 3_600 → expires_at = 4_600。
/// 预期结果：正断言：expires_at == 4_600；令牌与 profile 原样带入。
#[test]
fn test_token_response_into_session_computes_expiry() {
    let response = TokenResponse {
        schema_version: SESSION_SCHEMA_VERSION,
        access_token: "a".to_string(),
        refresh_token: "r".to_string(),
        expires_in: 3_600,
        profile: profile(Provider::Google, "9"),
    };
    let session = response.into_session(1_000).expect("当前版本应可转换");
    assert_eq!(session.expires_at, 4_600, "到期时间应为 now + expires_in");
    assert_eq!(session.access_token, "a", "access token 原样带入");
    assert_eq!(
        session.profile.primary_provider(),
        Some(Provider::Google),
        "profile 原样带入"
    );
}

/// 服务端返回更高 schema 版本时拒绝，避免误读新字段。
///
/// 预期结果：负断言：Err(UnsupportedVersion)。
#[test]
fn test_token_response_rejects_future_schema() {
    let response = TokenResponse {
        schema_version: SESSION_SCHEMA_VERSION + 1,
        access_token: "a".to_string(),
        refresh_token: "r".to_string(),
        expires_in: 1,
        profile: profile(Provider::GitHub, "1"),
    };
    assert_eq!(
        response.into_session(0),
        Err(SessionDecodeError::UnsupportedVersion(
            SESSION_SCHEMA_VERSION + 1
        )),
        "未知更高版本必须拒绝"
    );
}
