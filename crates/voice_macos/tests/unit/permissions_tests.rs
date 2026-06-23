use super::*;

/// VM-P-1: Apple speech authorization raw values should map to provider states.
///
/// 数据构造（含关键数值的推导过程）：
///   Apple SFSpeechRecognizerAuthorizationStatus raw values:
///   NotDetermined = 0, Denied = 1, Restricted = 2, Authorized = 3
///   v1 provider exposes the same four-state domain enum
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Convert raw 0 into SpeechAuthorization::NotDetermined
///   2. Convert raw 1 into SpeechAuthorization::Denied
///   3. Convert raw 2 into SpeechAuthorization::Restricted
///   4. Convert raw 3 into SpeechAuthorization::Authorized
///
/// 预期结果：
///   - 正断言：authorized raw value maps to Authorized
///   - 正断言：denied/restricted preserve their exact failure states
///   - 负断言：unknown raw value must not map to Authorized
#[test]
fn test_speech_authorization_raw_mapping_is_exact() {
    assert_eq!(
        speech_authorization_from_raw(0),
        Some(SpeechAuthorization::NotDetermined),
        "raw 0 should map to speech NotDetermined"
    );
    assert_eq!(
        speech_authorization_from_raw(1),
        Some(SpeechAuthorization::Denied),
        "raw 1 should map to speech Denied"
    );
    assert_eq!(
        speech_authorization_from_raw(2),
        Some(SpeechAuthorization::Restricted),
        "raw 2 should map to speech Restricted"
    );
    assert_eq!(
        speech_authorization_from_raw(3),
        Some(SpeechAuthorization::Authorized),
        "raw 3 should map to speech Authorized"
    );
    assert_ne!(
        speech_authorization_from_raw(99),
        Some(SpeechAuthorization::Authorized),
        "unknown raw values must not be treated as speech Authorized"
    );
}

/// VM-P-2: Apple microphone authorization raw values should map to provider states.
///
/// 数据构造（含关键数值的推导过程）：
///   Apple AVAuthorizationStatus raw values:
///   NotDetermined = 0, Restricted = 1, Denied = 2, Authorized = 3
///   Denied and Restricted differ from Speech raw ordering and must not be swapped
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Convert raw 0 into MicrophoneAuthorization::NotDetermined
///   2. Convert raw 1 into MicrophoneAuthorization::Restricted
///   3. Convert raw 2 into MicrophoneAuthorization::Denied
///   4. Convert raw 3 into MicrophoneAuthorization::Authorized
///
/// 预期结果：
///   - 正断言：raw 1 maps to Restricted, not Denied
///   - 正断言：raw 2 maps to Denied, not Restricted
///   - 负断言：unknown raw value must not map to Authorized
#[test]
fn test_microphone_authorization_raw_mapping_uses_av_order() {
    assert_eq!(
        microphone_authorization_from_raw(0),
        Some(MicrophoneAuthorization::NotDetermined),
        "raw 0 should map to microphone NotDetermined"
    );
    assert_eq!(
        microphone_authorization_from_raw(1),
        Some(MicrophoneAuthorization::Restricted),
        "AV raw 1 should map to microphone Restricted"
    );
    assert_eq!(
        microphone_authorization_from_raw(2),
        Some(MicrophoneAuthorization::Denied),
        "AV raw 2 should map to microphone Denied"
    );
    assert_eq!(
        microphone_authorization_from_raw(3),
        Some(MicrophoneAuthorization::Authorized),
        "raw 3 should map to microphone Authorized"
    );
    assert_ne!(
        microphone_authorization_from_raw(99),
        Some(MicrophoneAuthorization::Authorized),
        "unknown raw values must not be treated as microphone Authorized"
    );
}
