use super::*;
use crate::SpeechError;

/// VM-T-1: final transcript should win over earlier partial transcript.
///
/// 数据构造（含关键数值的推导过程）：
///   partial = "创建一个 Codex"
///   final = "创建一个 Codex 卡片"
///   final contains the complete utterance and should be preferred
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Push partial transcript
///   2. Push final transcript
///   3. Finish accumulator
///
/// 预期结果：
///   - 正断言：finish returns final transcript exactly
///   - 正断言：returned text contains "卡片"
///   - 负断言：finish must not return the shorter partial
#[test]
fn test_final_transcript_wins_over_partial() {
    let mut transcript = TranscriptAccumulator::default();

    transcript.push_partial("创建一个 Codex");
    transcript.push_final("创建一个 Codex 卡片");
    let finished = transcript.finish().expect("final transcript should finish");

    assert_eq!(
        finished, "创建一个 Codex 卡片",
        "final transcript should be preferred over partial"
    );
    assert!(
        finished.contains("卡片"),
        "returned transcript should include the final-only token"
    );
    assert_ne!(
        finished, "创建一个 Codex",
        "final transcript should not be replaced by the earlier partial"
    );
}

/// VM-T-2: last non-empty partial transcript should be used when no final arrives.
///
/// 数据构造（含关键数值的推导过程）：
///   first partial = "创建终端"
///   empty partial = "   " and should be ignored
///   second partial = "创建终端 并输入 ls"
///   final = None, so fallback should use the latest non-empty partial
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Push first partial
///   2. Push whitespace-only partial
///   3. Push second partial
///   4. Finish without final
///
/// 预期结果：
///   - 正断言：finish returns latest non-empty partial
///   - 正断言：returned text contains "ls"
///   - 负断言：empty partial should not erase prior content
#[test]
fn test_latest_partial_is_used_when_final_is_missing() {
    let mut transcript = TranscriptAccumulator::default();

    transcript.push_partial("创建终端");
    transcript.push_partial("   ");
    transcript.push_partial("创建终端 并输入 ls");
    let finished = transcript
        .finish()
        .expect("latest non-empty partial should finish");

    assert_eq!(
        finished, "创建终端 并输入 ls",
        "latest non-empty partial should be the fallback transcript"
    );
    assert!(
        finished.contains("ls"),
        "fallback transcript should include ls"
    );
    assert_ne!(
        finished, "   ",
        "whitespace partial must not erase usable transcript text"
    );
}

/// VM-T-3: empty accumulator should return EmptyTranscript and no text.
///
/// 数据构造（含关键数值的推导过程）：
///   partial count = 0
///   final count = 0
///   usable transcript length = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Create default accumulator
///   2. Finish immediately
///   3. Inspect error
///
/// 预期结果：
///   - 正断言：error is SpeechError::EmptyTranscript
///   - 负断言：finish must not return Ok("")
#[test]
fn test_empty_transcript_returns_empty_error() {
    let transcript = TranscriptAccumulator::default();

    let error = transcript
        .finish()
        .expect_err("empty transcript should fail");

    assert_eq!(
        error,
        SpeechError::EmptyTranscript,
        "empty accumulator should return the precise EmptyTranscript error"
    );
    assert_ne!(
        Ok::<String, SpeechError>("".to_string()),
        Err(error),
        "empty transcript must not be returned as successful text"
    );
}
