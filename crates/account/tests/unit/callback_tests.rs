use super::*;

/// 合法回调返回 code。
///
/// 数据构造：目标 "/callback?code=abc&state=xyz"，期望 state = "xyz"。
/// 预期结果：正断言：Ok("abc")。
#[test]
fn test_valid_callback_returns_code() {
    assert_eq!(
        parse_callback("/callback?code=abc&state=xyz", "xyz"),
        Ok("abc".to_string()),
        "state 匹配且带 code 时应返回 code"
    );
}

/// 查询参数按百分号解码。
///
/// 数据构造：code=a%2Bb%2Fc → "a+b/c"。
/// 预期结果：正断言：解码后为 "a+b/c"；负断言：非法转义 "%zz" 视为 Malformed。
#[test]
fn test_callback_percent_decodes() {
    assert_eq!(
        parse_callback("/callback?code=a%2Bb%2Fc&state=s", "s"),
        Ok("a+b/c".to_string()),
        "%2B/%2F 应解码为 + 和 /"
    );
    assert_eq!(
        parse_callback("/callback?code=a%zz&state=s", "s"),
        Err(CallbackError::Malformed),
        "非法百分号转义必须拒绝"
    );
}

/// state 缺失或不匹配一律拒绝，且优先于 error/code 判断。
///
/// 数据构造：期望 state = "good"。
/// 预期结果：
///   - 负断言：缺 state → MissingState；错误 state → StateMismatch
///   - 负断言：state 错误且带 error 参数时仍是 StateMismatch（不回显攻击者构造的错误）
#[test]
fn test_callback_state_checked_first() {
    assert_eq!(
        parse_callback("/callback?code=abc", "good"),
        Err(CallbackError::MissingState),
        "缺少 state 必须拒绝"
    );
    assert_eq!(
        parse_callback("/callback?code=abc&state=evil", "good"),
        Err(CallbackError::StateMismatch),
        "state 不匹配必须拒绝"
    );
    assert_eq!(
        parse_callback("/callback?error=access_denied&state=evil", "good"),
        Err(CallbackError::StateMismatch),
        "state 错误时不应走到 error 分支"
    );
}

/// 用户在提供方拒绝授权。
///
/// 数据构造："/callback?error=access_denied&state=s"，期望 state = "s"。
/// 预期结果：Err(ProviderDenied("access_denied"))。
#[test]
fn test_callback_provider_denied() {
    assert_eq!(
        parse_callback("/callback?error=access_denied&state=s", "s"),
        Err(CallbackError::ProviderDenied("access_denied".to_string())),
        "提供方返回 error 时应上报拒绝原因"
    );
}

/// code 缺失、为空，或路径不是 /callback。
///
/// 预期结果：负断言：缺 code、空 code → MissingCode；路径 /other → Malformed。
#[test]
fn test_callback_rejects_missing_code_and_wrong_path() {
    assert_eq!(
        parse_callback("/callback?state=s", "s"),
        Err(CallbackError::MissingCode),
        "没有 code 必须拒绝"
    );
    assert_eq!(
        parse_callback("/callback?code=&state=s", "s"),
        Err(CallbackError::MissingCode),
        "空 code 必须拒绝"
    );
    assert_eq!(
        parse_callback("/other?code=abc&state=s", "s"),
        Err(CallbackError::Malformed),
        "非 /callback 路径必须拒绝"
    );
}
