use crate::report_queue::{decide_template_report, ReportPlayDecision};
use crate::session::DialoguePhase;

/// RQ-1: Speaking 阶段模板播报应排队（Queue）。
///
/// 数据构造：
///   phase=Speaking，user_speech_active=false。
///
/// 预期结果：
///   - 正断言：决策为 Queue
///   - 负断言：决策不是 PlayNow
#[test]
fn test_speaking_phase_queues_template_report() {
    let decision = decide_template_report(DialoguePhase::Speaking, false);
    assert_eq!(
        decision,
        ReportPlayDecision::Queue,
        "Speaking 且用户未说话时模板播报应排队"
    );
    assert_ne!(
        decision,
        ReportPlayDecision::PlayNow,
        "Speaking 时不应立即播放"
    );
}

/// RQ-2: 用户正在说话时，无论 phase 如何都应延后（DeferWhileUserSpeaking）。
///
/// 数据构造：
///   user_speech_active=true，分别测试 Listening / Thinking / Speaking / Idle。
///
/// 预期结果：
///   - 正断言：各 phase 均返回 DeferWhileUserSpeaking
#[test]
fn test_user_speech_active_defers_template_report() {
    for phase in [
        DialoguePhase::Idle,
        DialoguePhase::Listening,
        DialoguePhase::Thinking,
        DialoguePhase::Speaking,
    ] {
        let decision = decide_template_report(phase, true);
        assert_eq!(
            decision,
            ReportPlayDecision::DeferWhileUserSpeaking,
            "user_speech_active=true 时 phase={phase:?} 应延后播报"
        );
    }
}

/// RQ-3: Listening 阶段且用户未说话时应立即播放（PlayNow）。
///
/// 预期结果：
///   - 正断言：PlayNow
///   - 负断言：不是 Queue 或 Defer
#[test]
fn test_listening_phase_plays_template_report_now() {
    let decision = decide_template_report(DialoguePhase::Listening, false);
    assert_eq!(
        decision,
        ReportPlayDecision::PlayNow,
        "Listening 且用户未说话时应立即播放模板播报"
    );
    assert_ne!(decision, ReportPlayDecision::Queue, "Listening 时不应排队");
    assert_ne!(
        decision,
        ReportPlayDecision::DeferWhileUserSpeaking,
        "用户未说话时不应延后"
    );
}

/// RQ-4: Thinking 阶段模板播报应排队（Queue）。
///
/// 预期结果：
///   - 正断言：Queue
#[test]
fn test_thinking_phase_queues_template_report() {
    let decision = decide_template_report(DialoguePhase::Thinking, false);
    assert_eq!(
        decision,
        ReportPlayDecision::Queue,
        "Thinking 时模板播报应排队，等待 LLM/回复流程"
    );
}

/// RQ-5: Idle 阶段且用户未说话时应立即播放（PlayNow）。
///
/// 预期结果：
///   - 正断言：PlayNow
#[test]
fn test_idle_phase_plays_template_report_now() {
    let decision = decide_template_report(DialoguePhase::Idle, false);
    assert_eq!(
        decision,
        ReportPlayDecision::PlayNow,
        "Idle 且用户未说话时应立即播放模板播报"
    );
}
