use super::*;

/// RFC 7636 附录 B 的官方示例向量。
///
/// 数据构造：verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"，
/// 期望 challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"（S256）。
/// 执行过程：调用 `challenge_for(verifier)`。
/// 预期结果：
///   - 正断言：与 RFC 向量逐字符相等
///   - 负断言：不等于 verifier 本身（不是 plain 方法）
#[test]
fn test_challenge_matches_rfc7636_vector() {
    let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    let challenge = challenge_for(verifier);
    assert_eq!(
        challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
        "S256 challenge 应与 RFC 7636 附录 B 一致"
    );
    assert_ne!(challenge, verifier, "不得退化为 plain 方法");
}

/// 由 32 字节熵生成 43 字符 base64url verifier。
///
/// 数值推导：32 字节 = 256 位；base64url 无填充长度 = ceil(32*4/3) = 43，
/// 落在 RFC 要求的 43–128 之内。全零熵编码为 43 个 'A'。
/// 预期结果：
///   - 正断言：长度 43，字符均属 [A-Za-z0-9_-]；全零熵得到 43 个 'A'；challenge 与 verifier 匹配
///   - 负断言：不同熵得到不同 verifier
#[test]
fn test_from_entropy_builds_valid_verifier() {
    let zero = Pkce::from_entropy(&[0u8; 32]);
    assert_eq!(zero.verifier().len(), 43, "32 字节熵应编码为 43 字符");
    assert_eq!(zero.verifier(), "A".repeat(43), "全零熵应编码为 43 个 'A'");
    assert!(
        zero.verifier()
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "verifier 只能含 base64url 字符"
    );
    assert_eq!(
        zero.challenge(),
        challenge_for(zero.verifier()),
        "challenge 必须由 verifier 派生"
    );
    let other = Pkce::from_entropy(&[1u8; 32]);
    assert_ne!(
        zero.verifier(),
        other.verifier(),
        "不同熵必须得到不同 verifier"
    );
}

/// 系统随机源每次生成不同的 PKCE 与 state。
///
/// 预期结果：
///   - 负断言：两次 `Pkce::generate()` 的 verifier 不同；两次 `random_state()` 不同
///   - 正断言：state 长度 43（256 位熵，≥ 规格要求的 128 位）
#[test]
fn test_generate_is_random() {
    assert_ne!(
        Pkce::generate().verifier(),
        Pkce::generate().verifier(),
        "两次生成的 verifier 不应相同"
    );
    let (a, b) = (random_state(), random_state());
    assert_ne!(a, b, "两次生成的 state 不应相同");
    assert_eq!(a.len(), 43, "state 应为 32 字节熵的 base64url 编码");
}

/// state 校验需完整相等，且拒绝空值与长度不同的输入。
///
/// 数据构造：expected = "abc123"。
/// 预期结果：
///   - 正断言：相同字符串通过
///   - 负断言：末位不同、前缀、超长、空串均不通过
#[test]
fn test_verify_state_rejects_mismatches() {
    assert!(verify_state("abc123", "abc123"), "相同 state 应通过");
    assert!(!verify_state("abc123", "abc124"), "末位不同必须拒绝");
    assert!(!verify_state("abc123", "abc12"), "前缀不能通过");
    assert!(!verify_state("abc123", "abc1234"), "超长不能通过");
    assert!(!verify_state("abc123", ""), "空 state 必须拒绝");
    assert!(!verify_state("", ""), "期望值为空时也必须拒绝");
}

/// Debug 输出不得泄露 verifier。
///
/// 预期结果：负断言：`format!("{:?}")` 不含 verifier 原文。
#[test]
fn test_pkce_debug_redacts_verifier() {
    let pkce = Pkce::from_entropy(&[7u8; 32]);
    let text = format!("{pkce:?}");
    assert!(
        !text.contains(pkce.verifier()),
        "Debug 不得包含 verifier，避免进入日志"
    );
}
