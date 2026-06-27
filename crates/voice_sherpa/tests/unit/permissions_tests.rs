use super::{microphone_authorization_from_raw, MicrophoneAuthorization};

/// PERM-1: Apple AVAuthorizationStatus raw values map to MicrophoneAuthorization variants.
///
/// 数据构造（含关键数值的推导过程）：
///   Apple AVAuthorizationStatus raw values (from AVCaptureDevice):
///   NotDetermined = 0, Restricted = 1, Denied = 2, Authorized = 3
///   Note: Denied and Restricted use the AV ordering (differs from SFSpeechRecognizer).
///   Unknown value (99) is out of range and must return None.
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Pass raw 0 → expect NotDetermined
///   2. Pass raw 1 → expect Restricted (AV order: Restricted < Denied)
///   3. Pass raw 2 → expect Denied
///   4. Pass raw 3 → expect Authorized
///   5. Pass raw 99 → expect None (unrecognized, must not become Authorized)
///
/// 预期结果：
///   - 正断言：each valid raw value maps to exactly the matching variant
///   - 正断言：raw 1 maps to Restricted, not Denied (AV ordering preserved)
///   - 正断言：raw 2 maps to Denied, not Restricted
///   - 负断言：raw 99 returns None, not Some(Authorized)
#[test]
fn test_microphone_authorization_from_raw_maps_all_variants() {
    assert_eq!(
        microphone_authorization_from_raw(0),
        Some(MicrophoneAuthorization::NotDetermined),
        "raw 0 should be NotDetermined"
    );
    assert_eq!(
        microphone_authorization_from_raw(1),
        Some(MicrophoneAuthorization::Restricted),
        "AV raw 1 should be Restricted (not Denied — AV ordering differs from SF)"
    );
    assert_eq!(
        microphone_authorization_from_raw(2),
        Some(MicrophoneAuthorization::Denied),
        "AV raw 2 should be Denied (not Restricted)"
    );
    assert_eq!(
        microphone_authorization_from_raw(3),
        Some(MicrophoneAuthorization::Authorized),
        "raw 3 should be Authorized"
    );
    assert_eq!(
        microphone_authorization_from_raw(99),
        None,
        "raw 99 should be unrecognized — must not silently become Authorized"
    );
    // Negative: verify raw 1 is not Denied (would allow a denied mic to appear as restricted)
    assert_ne!(
        microphone_authorization_from_raw(1),
        Some(MicrophoneAuthorization::Denied),
        "raw 1 must not map to Denied — that would misreport restricted devices"
    );
    // Negative: verify raw 2 is not Restricted
    assert_ne!(
        microphone_authorization_from_raw(2),
        Some(MicrophoneAuthorization::Restricted),
        "raw 2 must not map to Restricted — that would misreport denied devices"
    );
    // Negative: unknown value must not map to Authorized
    assert_ne!(
        microphone_authorization_from_raw(99),
        Some(MicrophoneAuthorization::Authorized),
        "unknown raw values must not be treated as Authorized"
    );
}
