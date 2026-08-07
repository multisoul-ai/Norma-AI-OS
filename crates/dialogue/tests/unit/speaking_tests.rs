use crate::session::{DialoguePhase, DialogueSession};
use crate::speaking::{
    format_stub_reply, handle_barge_in_speech_start, handle_speaking_speech_start,
    SpeakingSpeechSource,
};

/// SPK-1: Speaking 阶段收到 SpeechStart 应回到 Listening 且 models_resident 不变。
///
/// 数据构造：
///   会话经 Listening → Thinking → Speaking。
///
/// 执行过程：
///   1. handle_barge_in_speech_start
///
/// 预期结果：
///   - 正断言：返回 true
///   - 正断言：phase == Listening
///   - 正断言：models_resident 仍为 true
#[test]
fn test_barge_in_speech_start_returns_to_listening() {
    let mut session = DialogueSession::new();
    session.on_listening_started();
    session.on_utterance_final().expect("应进入 Thinking");
    session.on_reply_ready().expect("应进入 Speaking");
    assert!(session.models_resident(), "前置：models_resident 应为 true");

    assert!(
        handle_barge_in_speech_start(&mut session),
        "Speaking 下 barge-in 应被处理"
    );

    assert_eq!(
        session.phase(),
        DialoguePhase::Listening,
        "barge-in 后应回到 Listening"
    );
    assert!(
        session.models_resident(),
        "barge-in 不应卸载 models_resident"
    );
}

/// SPK-2: 非 Speaking 阶段忽略 SpeechStart，phase 不变。
///
/// 预期结果：
///   - 负断言：Listening 下返回 false
///   - 正断言：phase 仍为 Listening
#[test]
fn test_barge_in_ignored_outside_speaking() {
    let mut session = DialogueSession::new();
    session.on_listening_started();

    assert!(
        !handle_barge_in_speech_start(&mut session),
        "Listening 下不应触发 barge-in"
    );
    assert_eq!(
        session.phase(),
        DialoguePhase::Listening,
        "非 Speaking 阶段 phase 不应改变"
    );
}

/// SPK-3: stub reply 模板应包含用户 transcript。
#[test]
fn test_format_stub_reply_prefixes_transcript() {
    assert_eq!(
        format_stub_reply("你好"),
        "收到：你好",
        "stub reply 应使用收到：前缀包裹 transcript"
    );
}

/// SPK-4: 未分类麦克风 SpeechStart 可能是 assistant 回声，不得直接截断 Speaking。
///
/// 数据构造：
///   会话经 Listening → Thinking → Speaking；source = UnclassifiedMicrophone。
///
/// 执行过程：
///   1. handle_speaking_speech_start(session, UnclassifiedMicrophone)
///
/// 预期结果：
///   - 负断言：返回 false，不触发 barge-in；
///   - 正断言：phase 仍为 Speaking，完整 TTS 可继续播放。
#[test]
fn test_unclassified_microphone_speech_does_not_interrupt_speaking() {
    let mut session = DialogueSession::new();
    session.on_listening_started();
    session.on_utterance_final().expect("应进入 Thinking");
    session.on_reply_ready().expect("应进入 Speaking");

    let interrupted =
        handle_speaking_speech_start(&mut session, SpeakingSpeechSource::UnclassifiedMicrophone);

    assert!(
        !interrupted,
        "未分类麦克风事件可能是 TTS 回声，不应直接打断"
    );
    assert_eq!(
        session.phase(),
        DialoguePhase::Speaking,
        "忽略回声候选后应继续 Speaking"
    );
}
