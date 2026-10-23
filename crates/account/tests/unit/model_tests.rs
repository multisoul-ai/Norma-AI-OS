use super::fixtures::{profile, session};
use super::*;

/// 会话编码后可无损解码。
///
/// 数据构造：expires_at = 2_000，GitHub 身份 subject = "42"。
/// 预期结果：正断言：decode(encode(s)) == s；编码结果带当前 schema_version。
#[test]
fn test_session_round_trips_through_json() {
    let original = session(2_000);
    let raw = original.encode();
    assert!(
        raw.contains(&format!("\"schema_version\":{SESSION_SCHEMA_VERSION}")),
        "编码结果必须带 schema_version"
    );
    assert_eq!(
        Session::decode(&raw),
        Ok(original),
        "编码再解码应得到相同会话"
    );
}

/// 过期判断的边界：到点即过期。
///
/// 数值推导：expires_at = 1_000。now=999 → 未过期；now=1_000 → 已过期；now=1_001 → 已过期。
/// 预期结果：三个断言分别覆盖边界前、边界、边界后。
#[test]
fn test_session_expiry_boundary() {
    let s = session(1_000);
    assert!(!s.is_expired(999), "到期前 1 秒不应过期");
    assert!(s.is_expired(1_000), "到期时刻即视为过期");
    assert!(s.is_expired(1_001), "到期后必须过期");
}

/// 未来版本与损坏数据被拒绝，调用方据此回落为未登录。
///
/// 数据构造：把合法 JSON 的 schema_version 改为当前版本 + 1；另给出非 JSON 与缺字段输入。
/// 预期结果：
///   - 负断言：更高版本 → UnsupportedVersion(当前+1)；乱码与缺字段 → Malformed
#[test]
fn test_decode_rejects_unknown_version_and_garbage() {
    let future = session(5).encode().replace(
        &format!("\"schema_version\":{SESSION_SCHEMA_VERSION}"),
        &format!("\"schema_version\":{}", SESSION_SCHEMA_VERSION + 1),
    );
    assert_eq!(
        Session::decode(&future),
        Err(SessionDecodeError::UnsupportedVersion(
            SESSION_SCHEMA_VERSION + 1
        )),
        "更高版本的会话必须拒绝，避免误读"
    );
    assert_eq!(
        Session::decode("not json"),
        Err(SessionDecodeError::Malformed),
        "非 JSON 必须拒绝"
    );
    assert_eq!(
        Session::decode("{\"schema_version\":1}"),
        Err(SessionDecodeError::Malformed),
        "缺少必需字段必须拒绝"
    );
}

/// Debug 输出不得包含令牌。
///
/// 预期结果：负断言：不含 access/refresh 令牌原文。
#[test]
fn test_session_debug_redacts_tokens() {
    let text = format!("{:?}", session(1));
    assert!(
        !text.contains("access-secret"),
        "Debug 不得泄露 access token"
    );
    assert!(
        !text.contains("refresh-secret"),
        "Debug 不得泄露 refresh token"
    );
}

/// 同邮箱不同提供方是两个独立身份，登录来源取第一项。
///
/// 数据构造：GitHub 与 Google 的 profile 邮箱相同，subject 不同。
/// 预期结果：
///   - 正断言：primary_provider 分别为 GitHub 与 Google
///   - 负断言：两个 Identity 不相等（邮箱不参与身份）；空身份列表返回 None
#[test]
fn test_identity_is_provider_and_subject_not_email() {
    let gh = profile(Provider::GitHub, "1");
    let gg = profile(Provider::Google, "1");
    assert_eq!(gh.email, gg.email, "测试前提：两个 profile 邮箱相同");
    assert_eq!(
        gh.primary_provider(),
        Some(Provider::GitHub),
        "GitHub 登录来源"
    );
    assert_eq!(
        gg.primary_provider(),
        Some(Provider::Google),
        "Google 登录来源"
    );
    assert_ne!(
        gh.identities[0], gg.identities[0],
        "同邮箱不同提供方不是同一身份"
    );
    let mut empty = gh;
    empty.identities.clear();
    assert_eq!(empty.primary_provider(), None, "没有身份时不应有登录来源");
}
