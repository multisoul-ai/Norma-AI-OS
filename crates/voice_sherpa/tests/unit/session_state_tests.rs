use crate::session_state::{SessionEvent, SessionPhase, SessionState};
use crate::SpeechError;

/// SS-1: Normal flow Idle → Recording → Finished
///
/// Execution:
///   1. Start event → Idle → Recording
///   2. Finish event → Recording → Finished
///
/// Expected results:
///   - Both transitions return Ok
///   - Final phase == Finished
#[test]
fn test_session_state_normal_flow() {
    let mut state = SessionState::default();
    assert_eq!(
        state.apply(SessionEvent::Start),
        Ok(()),
        "Start from Idle should succeed"
    );
    assert_eq!(
        state.phase(),
        SessionPhase::Recording,
        "should be Recording after Start"
    );
    assert_eq!(
        state.apply(SessionEvent::Finish),
        Ok(()),
        "Finish from Recording should succeed"
    );
    assert_eq!(
        state.phase(),
        SessionPhase::Finished,
        "should be Finished after Finish"
    );
}

/// SS-2: Double Finish from Finished state → AlreadyFinished
///
/// Expected results:
///   - Second Finish returns Err(AlreadyFinished)
#[test]
fn test_session_state_double_finish_is_error() {
    let mut state = SessionState::default();
    state.apply(SessionEvent::Start).unwrap();
    state.apply(SessionEvent::Finish).unwrap();
    assert_eq!(
        state.apply(SessionEvent::Finish),
        Err(SpeechError::AlreadyFinished),
        "double finish should return AlreadyFinished"
    );
}

/// SS-3: Finish from Idle state → NotRecording
///
/// Expected results:
///   - Err(NotRecording)
#[test]
fn test_session_state_finish_without_start_is_error() {
    let mut state = SessionState::default();
    assert_eq!(
        state.apply(SessionEvent::Finish),
        Err(SpeechError::NotRecording),
        "finish without start should return NotRecording"
    );
}

/// SS-4: Duplicate Start while already Recording returns AlreadyFinished, not NotRecording.
///
/// Data:
///   Initial state: Idle → Recording (via first Start)
///
/// Execution:
///   1. apply(Idle, Start) → Recording (ok)
///   2. apply(Recording, Start) → error
///
/// Expected:
///   - Positive: error is Err(SpeechError::AlreadyFinished)
///   - Negative: error is NOT Err(SpeechError::NotRecording)
#[test]
fn test_session_state_duplicate_start_returns_already_finished() {
    let mut state = SessionState::default();
    let first = state.apply(SessionEvent::Start);
    assert!(first.is_ok(), "first Start should succeed");
    let second = state.apply(SessionEvent::Start);
    assert_eq!(
        second,
        Err(SpeechError::AlreadyFinished),
        "duplicate Start should return AlreadyFinished, not NotRecording"
    );
    assert_ne!(
        second,
        Err(SpeechError::NotRecording),
        "AlreadyFinished is a more accurate error than NotRecording for a duplicate Start"
    );
}
