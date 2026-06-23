use super::*;
use crate::SpeechError;

/// VM-S-1: session state should allow exactly one start from Idle.
///
/// 数据构造（含关键数值的推导过程）：
///   initial phase = Idle
///   first event = Start
///   second event = Start again while Recording
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Apply Start to Idle
///   2. Apply Start again to Recording
///   3. Inspect both results and final phase
///
/// 预期结果：
///   - 正断言：first Start succeeds and phase becomes Recording
///   - 正断言：second Start returns AlreadyFinished-equivalent provider error
///   - 负断言：phase should not return to Idle after duplicate Start
#[test]
fn test_session_state_rejects_duplicate_start() {
    let mut state = SessionState::default();

    let first = state.apply(SessionEvent::Start);
    let duplicate = state.apply(SessionEvent::Start);

    assert_eq!(first, Ok(()), "first Start from Idle should succeed");
    assert_eq!(
        duplicate,
        Err(SpeechError::Provider(
            "speech session is already recording".to_string()
        )),
        "duplicate Start should return a precise provider error"
    );
    assert_eq!(
        state.phase(),
        SessionPhase::Recording,
        "duplicate Start should leave session in Recording"
    );
    assert_ne!(
        state.phase(),
        SessionPhase::Idle,
        "duplicate Start must not reset the session to Idle"
    );
}

/// VM-S-2: finish and cancel should be terminal states.
///
/// 数据构造（含关键数值的推导过程）：
///   path A = Idle -> Start -> Finish -> Finish
///   path B = Idle -> Start -> Cancel -> Finish
///   terminal states should reject further transitions
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Finish a recording session
///   2. Try to finish it again
///   3. Cancel a separate recording session
///   4. Try to finish the cancelled session
///
/// 预期结果：
///   - 正断言：first finish succeeds and phase becomes Finished
///   - 正断言：cancel succeeds and phase becomes Cancelled
///   - 负断言：finished/cancelled sessions must not return to Recording
#[test]
fn test_finish_and_cancel_are_terminal_states() {
    let mut finished = SessionState::default();
    let mut cancelled = SessionState::default();

    assert_eq!(
        finished.apply(SessionEvent::Start),
        Ok(()),
        "finished path should start recording"
    );
    assert_eq!(
        finished.apply(SessionEvent::Finish),
        Ok(()),
        "first Finish from Recording should succeed"
    );
    let duplicate_finish = finished.apply(SessionEvent::Finish);

    assert_eq!(
        cancelled.apply(SessionEvent::Start),
        Ok(()),
        "cancelled path should start recording"
    );
    assert_eq!(
        cancelled.apply(SessionEvent::Cancel),
        Ok(()),
        "Cancel from Recording should succeed"
    );
    let finish_after_cancel = cancelled.apply(SessionEvent::Finish);

    assert_eq!(
        finished.phase(),
        SessionPhase::Finished,
        "finished path should end in Finished"
    );
    assert_eq!(
        duplicate_finish,
        Err(SpeechError::AlreadyFinished),
        "Finishing an already finished session should return AlreadyFinished"
    );
    assert_eq!(
        cancelled.phase(),
        SessionPhase::Cancelled,
        "cancelled path should end in Cancelled"
    );
    assert_eq!(
        finish_after_cancel,
        Err(SpeechError::NotRecording),
        "Cancelled session cannot be finished because it is no longer recording"
    );
    assert_ne!(
        cancelled.phase(),
        SessionPhase::Recording,
        "cancelled session must not return to Recording"
    );
}
