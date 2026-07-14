use crate::{evaluate_local_license, LicenseInvalidReason, LocalLicenseStatus};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

#[path = "../support/token.rs"]
mod token_support;

use token_support::{generate_signing_key, public_key_hex, sign_token_json};

const MACHINE_ID: &str = "device-spike";

fn active_payload_json(exp: i64) -> String {
    format!(
        r#"{{"did":"{MACHINE_ID}","exp":{exp},"grc":0,"iat":1,"lid":"lic-1","nce":"n","pid":"p","pln":"pl","sts":"active"}}"#
    )
}

/// 有效 token 应离线验签通过并返回 license_id 与 exp。
///
/// 数据构造：
///   now = 1_700_000_000；exp = now + 3600（未过期）
///   payload.sts = "active"，payload.did = MACHINE_ID
///   使用本地 Ed25519 密钥对签名 base64url(payload) 字符串
///
/// 执行过程：
///   1. sign_token_json 构造 Keygate 格式 token
///   2. evaluate_local_license(Some(token), pubkey, MACHINE_ID, now)
///
/// 预期结果：
///   - 正断言：返回 Activated，license_id = Some("lic-1")，exp = Some(exp)
///   - 负断言：reason 不为 Missing/BadSignature/Expired
#[test]
fn test_valid_token_activates() {
    let signing_key = generate_signing_key();
    let pubkey = public_key_hex(&signing_key);
    let now = 1_700_000_000_i64;
    let exp = now + 3600;
    let token = sign_token_json(&active_payload_json(exp), &signing_key);

    let status = evaluate_local_license(Some(&token), &pubkey, MACHINE_ID, now);

    match status {
        LocalLicenseStatus::Activated {
            license_id,
            exp: got_exp,
        } => {
            assert_eq!(
                license_id.as_deref(),
                Some("lic-1"),
                "有效 token 应解析 payload.lid 为 license_id"
            );
            assert_eq!(got_exp, Some(exp), "有效 token 应保留 payload.exp");
        }
        LocalLicenseStatus::Invalid { reason } => {
            panic!("有效 token 不应无效，实际 reason={reason:?}");
        }
    }
}

/// 篡改 payload 后签名不匹配，应拒绝。
///
/// 数据构造：
///   先对 sts=active 的 payload 签名，再把 b64 段替换为 sts=revoked 的 payload（保留旧签名）
///
/// 执行过程：
///   evaluate_local_license 验签 base64 payload 字符串
///
/// 预期结果：
///   - 正断言：Invalid(BadSignature)
///   - 负断言：不是 Activated
#[test]
fn test_tampered_payload_rejected() {
    let signing_key = generate_signing_key();
    let pubkey = public_key_hex(&signing_key);
    let now = 1_700_000_000_i64;
    let exp = now + 3600;
    let token = sign_token_json(&active_payload_json(exp), &signing_key);
    let (_payload_b64, sig_b64) = token.rsplit_once('.').expect("token 应含分隔点");
    let revoked_json = format!(
        r#"{{"did":"{MACHINE_ID}","exp":{exp},"grc":0,"iat":1,"lid":"lic-1","nce":"n","pid":"p","pln":"pl","sts":"revoked"}}"#
    );
    let bad_b64 = URL_SAFE_NO_PAD.encode(revoked_json.as_bytes());
    let bad_token = format!("{bad_b64}.{sig_b64}");

    let status = evaluate_local_license(Some(&bad_token), &pubkey, MACHINE_ID, now);

    assert_eq!(
        status,
        LocalLicenseStatus::Invalid {
            reason: LicenseInvalidReason::BadSignature
        },
        "payload 被篡改后旧签名必须失效"
    );
}

/// 错误公钥无法验签通过。
///
/// 数据构造：
///   token 由 signing_key_a 签发；evaluate 传入 signing_key_b 的公钥 hex
///
/// 预期结果：
///   - 正断言：Invalid(BadSignature)
///   - 负断言：不是 Activated
#[test]
fn test_wrong_public_key_rejected() {
    let signing_key = generate_signing_key();
    let other_key = generate_signing_key();
    let wrong_pubkey = public_key_hex(&other_key);
    let now = 1_700_000_000_i64;
    let exp = now + 3600;
    let token = sign_token_json(&active_payload_json(exp), &signing_key);

    let status = evaluate_local_license(Some(&token), &wrong_pubkey, MACHINE_ID, now);

    assert_eq!(
        status,
        LocalLicenseStatus::Invalid {
            reason: LicenseInvalidReason::BadSignature
        },
        "公钥与签发密钥不匹配时必须 BadSignature"
    );
}

/// exp 早于 now_unix 时应过期拒绝。
///
/// 数据构造：
///   now = 1_700_000_000；exp = now - 1（已过期 1 秒）
///
/// 预期结果：
///   - 正断言：Invalid(Expired)
///   - 负断言：签名正确但仍不得 Activated
#[test]
fn test_expired_token_rejected() {
    let signing_key = generate_signing_key();
    let pubkey = public_key_hex(&signing_key);
    let now = 1_700_000_000_i64;
    let exp = now - 1;
    let token = sign_token_json(&active_payload_json(exp), &signing_key);

    let status = evaluate_local_license(Some(&token), &pubkey, MACHINE_ID, now);

    assert_eq!(
        status,
        LocalLicenseStatus::Invalid {
            reason: LicenseInvalidReason::Expired
        },
        "exp < now_unix 时必须 Expired"
    );
}

/// payload.did 与 machine_id 不一致时应拒绝。
///
/// 数据构造：
///   payload.did = "other-device"；machine_id = MACHINE_ID
///
/// 预期结果：
///   - 正断言：Invalid(DidMismatch)
///   - 负断言：不是 Activated
#[test]
fn test_did_mismatch_rejected() {
    let signing_key = generate_signing_key();
    let pubkey = public_key_hex(&signing_key);
    let now = 1_700_000_000_i64;
    let exp = now + 3600;
    let json = format!(
        r#"{{"did":"other-device","exp":{exp},"grc":0,"iat":1,"lid":"lic-1","nce":"n","pid":"p","pln":"pl","sts":"active"}}"#
    );
    let token = sign_token_json(&json, &signing_key);

    let status = evaluate_local_license(Some(&token), &pubkey, MACHINE_ID, now);

    assert_eq!(
        status,
        LocalLicenseStatus::Invalid {
            reason: LicenseInvalidReason::DidMismatch
        },
        "did 与 machine_id 不一致时必须 DidMismatch"
    );
}

/// 缺失 token（None 或空白）应 Missing。
///
/// 预期结果：
///   - 正断言：None → Missing；Some("") → Missing
///   - 负断言：不是 Activated
#[test]
fn test_missing_token_rejected() {
    let signing_key = generate_signing_key();
    let pubkey = public_key_hex(&signing_key);
    let now = 1_700_000_000_i64;

    let none_status = evaluate_local_license(None, &pubkey, MACHINE_ID, now);
    let blank_status = evaluate_local_license(Some("   "), &pubkey, MACHINE_ID, now);

    assert_eq!(
        none_status,
        LocalLicenseStatus::Invalid {
            reason: LicenseInvalidReason::Missing
        },
        "None token 应 Missing"
    );
    assert_eq!(
        blank_status,
        LocalLicenseStatus::Invalid {
            reason: LicenseInvalidReason::Missing
        },
        "空白 token 应 Missing"
    );
}

/// sts 存在且非 active 时应 BadStatus。
///
/// 数据构造：
///   payload.sts = "revoked"，签名有效，未过期，did 匹配
///
/// 预期结果：
///   - 正断言：Invalid(BadStatus)
///   - 负断言：不是 BadSignature
#[test]
fn test_non_active_status_rejected() {
    let signing_key = generate_signing_key();
    let pubkey = public_key_hex(&signing_key);
    let now = 1_700_000_000_i64;
    let exp = now + 3600;
    let json = format!(
        r#"{{"did":"{MACHINE_ID}","exp":{exp},"grc":0,"iat":1,"lid":"lic-1","nce":"n","pid":"p","pln":"pl","sts":"revoked"}}"#
    );
    let token = sign_token_json(&json, &signing_key);

    let status = evaluate_local_license(Some(&token), &pubkey, MACHINE_ID, now);

    assert_eq!(
        status,
        LocalLicenseStatus::Invalid {
            reason: LicenseInvalidReason::BadStatus
        },
        "sts 非 active 时必须 BadStatus"
    );
}
