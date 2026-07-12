use super::AsrUtteranceGate;
use dialogue::DialoguePhase;

/// DLG-ECHO-1：TTS 播放期间开始的回声 utterance，即使在播放结束后才产出 final 也不得提交。
///
/// 数据构造（含关键数值的推导过程）：
///   utterance 数量 = 2：第 1 段模拟 assistant TTS 回声，第 2 段模拟随后真实用户语音
///   phase 序列 = Speaking → Listening → Listening：回声开始、回声 final 延迟到达、真实用户开始
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Speaking 阶段收到第 1 段 SpeechStart → 标记该 utterance 为回声候选
///   2. TTS 播放结束，phase 回到 Listening 后收到第 1 段 final → 仍按 utterance 来源拒绝
///   3. Listening 阶段收到第 2 段 SpeechStart → 清除上一段抑制状态
///   4. 第 2 段 final 到达 → 允许作为真实用户输入提交
///
/// 预期结果：
///   - 负断言：跨越 Speaking → Listening 的 assistant 回声 final 不得提交
///   - 正断言：随后在 Listening 开始的真实用户 utterance final 应恢复提交
#[test]
fn test_tts_echo_final_is_rejected_after_phase_returns_to_listening() {
    let mut gate = AsrUtteranceGate::default();

    gate.on_speech_start(DialoguePhase::Speaking);
    assert!(
        !gate.take_final_acceptance(DialoguePhase::Listening),
        "TTS 回声在 Speaking 开始，即使 final 延迟到 Listening 才到达也不得提交"
    );

    gate.on_speech_start(DialoguePhase::Listening);
    assert!(
        gate.take_final_acceptance(DialoguePhase::Listening),
        "回声 final 被丢弃后，下一段在 Listening 开始的真实用户语音应恢复提交"
    );
}
