use crate::session::{DialoguePhase, DialogueSession};
use crate::DialogueError;

/// DS-1: 开始聆听后进入 Listening，且模型驻留标志为 true。
///
/// 数据构造：
///   新建 DialogueSession，初始 phase=Idle，models_resident=false。
///
/// 执行过程：
///   1. 调用 on_listening_started()
///
/// 预期结果：
///   - 正断言：phase == Listening
///   - 正断言：models_resident == true
///   - 负断言：phase != Idle
#[test]
fn test_listening_started_enters_listening_with_models_resident() {
    let mut session = DialogueSession::new();
    assert_eq!(session.phase(), DialoguePhase::Idle, "新会话应从 Idle 开始");
    assert!(
        !session.models_resident(),
        "新会话 models_resident 应为 false"
    );

    session.on_listening_started();

    assert_eq!(
        session.phase(),
        DialoguePhase::Listening,
        "on_listening_started 后应进入 Listening"
    );
    assert!(
        session.models_resident(),
        "开始聆听后 models_resident 应为 true"
    );
    assert_ne!(
        session.phase(),
        DialoguePhase::Idle,
        "开始聆听后不应停留在 Idle"
    );
}

/// DS-2: 停止聆听后回到 Idle，但 models_resident 仍为 true。
///
/// 数据构造：
///   会话先 on_listening_started()，models_resident=true。
///
/// 执行过程：
///   1. on_listening_stopped()
///
/// 预期结果：
///   - 正断言：phase == Idle
///   - 正断言：models_resident 仍为 true（模型不卸载）
#[test]
fn test_listening_stopped_returns_idle_keeps_models_resident() {
    let mut session = DialogueSession::new();
    session.on_listening_started();
    assert!(
        session.models_resident(),
        "前置：聆听中 models_resident 应为 true"
    );

    session.on_listening_stopped();

    assert_eq!(
        session.phase(),
        DialoguePhase::Idle,
        "停止聆听后应回到 Idle"
    );
    assert!(
        session.models_resident(),
        "停止聆听后 models_resident 应保持 true，模型继续驻留"
    );
}

/// DS-3: 窗口关闭后 models_resident 变为 false。
///
/// 数据构造：
///   会话曾 on_listening_started()，models_resident=true。
///
/// 执行过程：
///   1. on_window_closed()
///
/// 预期结果：
///   - 正断言：phase == Idle
///   - 正断言：models_resident == false
#[test]
fn test_window_closed_clears_models_resident() {
    let mut session = DialogueSession::new();
    session.on_listening_started();
    assert!(
        session.models_resident(),
        "前置：聆听中 models_resident 应为 true"
    );

    session.on_window_closed();

    assert_eq!(
        session.phase(),
        DialoguePhase::Idle,
        "关窗后 phase 应为 Idle"
    );
    assert!(
        !session.models_resident(),
        "关窗后 models_resident 应为 false，模型可卸载"
    );
}

/// DS-4: 完整一轮：final utterance → Thinking → reply ready → Speaking。
///
/// 执行过程：
///   1. on_listening_started() → Listening
///   2. on_utterance_final() → Thinking
///   3. on_reply_ready() → Speaking
///
/// 预期结果：
///   - 每步 Result 为 Ok
///   - 最终 phase == Speaking
#[test]
fn test_utterance_to_reply_enters_thinking_then_speaking() {
    let mut session = DialogueSession::new();
    session.on_listening_started();
    assert_eq!(
        session.phase(),
        DialoguePhase::Listening,
        "前置：应在 Listening"
    );

    assert_eq!(
        session.on_utterance_final(),
        Ok(()),
        "Listening 下 on_utterance_final 应成功"
    );
    assert_eq!(
        session.phase(),
        DialoguePhase::Thinking,
        "final utterance 后应进入 Thinking"
    );

    assert_eq!(
        session.on_reply_ready(),
        Ok(()),
        "Thinking 下 on_reply_ready 应成功"
    );
    assert_eq!(
        session.phase(),
        DialoguePhase::Speaking,
        "reply ready 后应进入 Speaking"
    );
}

/// DS-4b：后台任务报告只能在 Listening 时占用一次回复播放槽。
///
/// 数据构造：会话先进入 Listening，不产生用户 transcript。
/// 执行过程：调用 `on_report_pending()`。
/// 预期结果：成功进入 Thinking，随后可复用正常 `on_reply_ready()` 进入 Speaking。
#[test]
fn test_background_report_enters_thinking_before_playback() {
    let mut session = DialogueSession::new();
    session.on_listening_started();

    assert_eq!(
        session.on_report_pending(),
        Ok(()),
        "Listening 下后台终态报告应能占用回复播放槽"
    );
    assert_eq!(
        session.phase(),
        DialoguePhase::Thinking,
        "报告开始合成前应进入 Thinking"
    );
    assert_eq!(
        session.on_reply_ready(),
        Ok(()),
        "后台报告应复用正常 TTS ready 状态迁移"
    );
    assert_eq!(
        session.phase(),
        DialoguePhase::Speaking,
        "后台报告音频就绪后应进入 Speaking"
    );
}

/// DS-4c：Always Listening 未开启时，后台报告必须保留排队而不能擅自播放。
///
/// 数据构造：新会话保持 Idle。
/// 执行过程：调用 `on_report_pending()`。
/// 预期结果：返回 Err，phase 仍为 Idle。
#[test]
fn test_background_report_from_idle_is_rejected() {
    let mut session = DialogueSession::new();

    assert!(
        session.on_report_pending().is_err(),
        "Idle 下不得开始后台报告播放"
    );
    assert_eq!(
        session.phase(),
        DialoguePhase::Idle,
        "被拒绝的后台报告不得改变会话状态"
    );
}

/// DS-4d：明确来自单次语音指令的延迟报告可以从 Idle 占用回复播放槽。
///
/// 数据构造（含关键数值的推导过程）：
///   initial phase = Idle：单次语音的静默入队回复已经结束
///   deferred reports = 1：真实 Agent 派发回执
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 新建会话并保持 Idle
///   2. 调用 on_deferred_single_voice_report_pending
///   3. 调用 on_reply_ready 复用正常 TTS 状态迁移
///
/// 预期结果：
///   - 正断言：明确的延迟单次语音报告从 Idle 进入 Thinking，再进入 Speaking
///   - 负断言：通用 on_report_pending 的 Idle 拒绝规则仍由 DS-4c 保持
#[test]
fn test_deferred_single_voice_report_can_start_from_idle() {
    let mut session = DialogueSession::new();

    assert_eq!(
        session.on_deferred_single_voice_report_pending(),
        Ok(()),
        "明确来自单次语音的延迟报告必须可以从 Idle 占用播放槽"
    );
    assert_eq!(
        session.phase(),
        DialoguePhase::Thinking,
        "延迟单次语音报告开始合成前必须进入 Thinking"
    );
    assert_eq!(
        session.on_reply_ready(),
        Ok(()),
        "延迟单次语音报告必须复用正常的 TTS ready 迁移"
    );
    assert_eq!(
        session.phase(),
        DialoguePhase::Speaking,
        "延迟单次语音报告音频就绪后必须进入 Speaking"
    );
}

/// DS-5: 播放结束且 keep_listening=true 时回到 Listening。
///
/// 数据构造：
///   会话处于 Speaking（经 Listening → Thinking → Speaking）。
///
/// 执行过程：
///   1. on_playback_finished_or_interrupted(true)
///
/// 预期结果：
///   - 正断言：phase == Listening
#[test]
fn test_playback_finished_with_keep_listening_returns_to_listening() {
    let mut session = DialogueSession::new();
    session.on_listening_started();
    session.on_utterance_final().expect("应进入 Thinking");
    session.on_reply_ready().expect("应进入 Speaking");
    assert_eq!(
        session.phase(),
        DialoguePhase::Speaking,
        "前置：应在 Speaking"
    );

    session.on_playback_finished_or_interrupted(true);

    assert_eq!(
        session.phase(),
        DialoguePhase::Listening,
        "keep_listening=true 时播放结束应回到 Listening"
    );
}

/// DS-6: 播放结束且 keep_listening=false 时回到 Idle。
///
/// 执行过程：
///   1. 会话在 Speaking
///   2. on_playback_finished_or_interrupted(false)
///
/// 预期结果：
///   - 正断言：phase == Idle
///   - 负断言：phase != Listening
#[test]
fn test_playback_finished_without_keep_listening_returns_to_idle() {
    let mut session = DialogueSession::new();
    session.on_listening_started();
    session.on_utterance_final().expect("应进入 Thinking");
    session.on_reply_ready().expect("应进入 Speaking");

    session.on_playback_finished_or_interrupted(false);

    assert_eq!(
        session.phase(),
        DialoguePhase::Idle,
        "keep_listening=false 时播放结束应回到 Idle"
    );
    assert_ne!(
        session.phase(),
        DialoguePhase::Listening,
        "未保持聆听时不应回到 Listening"
    );
}

/// DS-7: 非 Listening 阶段调用 on_utterance_final 应返回 Err，不 panic。
///
/// 预期结果：
///   - 负断言：Idle 下 on_utterance_final 返回 Err
///   - 正断言：phase 仍为 Idle
#[test]
fn test_utterance_final_from_idle_returns_error() {
    let mut session = DialogueSession::new();
    assert_eq!(
        session.on_utterance_final(),
        Err(DialogueError::Message(
            "on_utterance_final requires Listening, got Idle".to_string()
        )),
        "Idle 下 on_utterance_final 应返回 Err"
    );
    assert_eq!(
        session.phase(),
        DialoguePhase::Idle,
        "非法转换后 phase 不应改变"
    );
}

/// DS-8: 非 Thinking 阶段调用 on_reply_ready 应返回 Err。
///
/// 预期结果：
///   - 负断言：Listening 下 on_reply_ready 返回 Err
#[test]
fn test_reply_ready_from_listening_returns_error() {
    let mut session = DialogueSession::new();
    session.on_listening_started();

    assert_eq!(
        session.on_reply_ready(),
        Err(DialogueError::Message(
            "on_reply_ready requires Thinking, got Listening".to_string()
        )),
        "Listening 下 on_reply_ready 应返回 Err"
    );
    assert_eq!(
        session.phase(),
        DialoguePhase::Listening,
        "非法转换后 phase 不应改变"
    );
}
