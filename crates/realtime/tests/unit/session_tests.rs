use super::{RealtimeSessionError, RealtimeSessionId, RealtimeSessionState, RealtimeStatus};

/// S-1: Realtime session state 从 Off 经过 Connecting/Listening 到 speaking/error 状态。
///
/// 数据构造（含关键数值的推导过程）：
///   session_id = "rt_1"（固定测试会话 id）
///   user_speaking flag = true（用户正在说话）
///   assistant_speaking flag = true（助手正在说话，显示优先级高于 user_speaking）
///   error message = "x"（最小非空错误文本，len = 1）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. RealtimeSessionState::default() → Off，且无 session_id
///   2. start_connecting("rt_1") → Connecting，并保存 session_id
///   3. mark_started() → Listening
///   4. mark_user_speaking(true) → UserSpeaking
///   5. mark_assistant_speaking(true) → AssistantSpeaking
///   6. mark_error("x") → Error，并保存错误文本
///
/// 预期结果：
///   - 正断言：每一步返回预期 display status 和关键字段
///   - 负断言：初始状态不是 Connecting，用户说话时不显示 AssistantSpeaking
#[test]
fn test_session_state_moves_through_realtime_statuses() {
    let mut state = RealtimeSessionState::default();
    assert_eq!(
        state.status(),
        RealtimeStatus::Off,
        "default realtime session state should be Off"
    );
    assert_eq!(
        state.session_id(),
        None,
        "default realtime session state should not have a session id"
    );
    assert_ne!(
        state.status(),
        RealtimeStatus::Connecting,
        "default realtime session state must not appear to be connecting"
    );

    state
        .start_connecting(RealtimeSessionId::new("rt_1"))
        .expect("starting from Off should be allowed");
    assert_eq!(
        state.status(),
        RealtimeStatus::Connecting,
        "start_connecting should move display status to Connecting"
    );
    assert_eq!(
        state.session_id(),
        Some(&RealtimeSessionId::new("rt_1")),
        "start_connecting should preserve the supplied session id"
    );

    state.mark_started();
    assert_eq!(
        state.status(),
        RealtimeStatus::Listening,
        "mark_started should move display status to Listening"
    );
    assert_ne!(
        state.status(),
        RealtimeStatus::Off,
        "started realtime session must no longer display Off"
    );

    state.mark_user_speaking(true);
    assert_eq!(
        state.status(),
        RealtimeStatus::UserSpeaking,
        "mark_user_speaking(true) should move display status to UserSpeaking"
    );
    assert_ne!(
        state.status(),
        RealtimeStatus::AssistantSpeaking,
        "user speaking alone should not display AssistantSpeaking"
    );

    state.mark_assistant_speaking(true);
    assert_eq!(
        state.status(),
        RealtimeStatus::AssistantSpeaking,
        "assistant speaking should take display priority when both speaking flags are true"
    );
    assert_ne!(
        state.status(),
        RealtimeStatus::UserSpeaking,
        "assistant speaking priority should prevent UserSpeaking from remaining visible"
    );

    state.mark_error("x");
    assert_eq!(
        state.status(),
        RealtimeStatus::Error,
        "mark_error should move display status to Error"
    );
    assert_eq!(
        state.error_message(),
        Some("x"),
        "mark_error should preserve the provided error message"
    );
    assert_ne!(
        state.status(),
        RealtimeStatus::Listening,
        "errored realtime session must not continue displaying Listening"
    );
}

/// S-2: active session 上重复 start_connecting 返回错误，并保留原 session id。
///
/// 数据构造（含关键数值的推导过程）：
///   existing_session_id = "rt_existing"（先进入 Connecting 的会话）
///   requested_session_id = "rt_new"（重复 start 请求）
///   active condition = status Connecting，此时已有 session_id
///
/// 执行过程（逐步说明系统如何处理）：
///   1. default state 调用 start_connecting("rt_existing") → Connecting
///   2. 再次调用 start_connecting("rt_new") → 返回 AlreadyActive
///   3. 检查 state 仍引用 rt_existing，状态仍为 Connecting
///
/// 预期结果：
///   - 正断言：错误中包含 existing_session_id = rt_existing
///   - 正断言：state.session_id 仍为 rt_existing
///   - 负断言：state.session_id 不应被替换为 rt_new
#[test]
fn test_start_connecting_while_active_returns_error_and_preserves_existing_session_id() {
    let mut state = RealtimeSessionState::default();
    state
        .start_connecting(RealtimeSessionId::new("rt_existing"))
        .expect("first start_connecting from Off should be allowed");

    let err = state
        .start_connecting(RealtimeSessionId::new("rt_new"))
        .expect_err("second start_connecting while Connecting should fail");

    assert_eq!(
        err,
        RealtimeSessionError::AlreadyActive {
            existing_session_id: RealtimeSessionId::new("rt_existing"),
        },
        "active duplicate start should report the existing session id"
    );
    assert_eq!(
        state.session_id(),
        Some(&RealtimeSessionId::new("rt_existing")),
        "duplicate start should preserve the original active session id"
    );
    assert_ne!(
        state.session_id(),
        Some(&RealtimeSessionId::new("rt_new")),
        "duplicate start must not replace the active session id with the requested id"
    );
    assert_eq!(
        state.status(),
        RealtimeStatus::Connecting,
        "duplicate start should not change the current Connecting status"
    );
}
