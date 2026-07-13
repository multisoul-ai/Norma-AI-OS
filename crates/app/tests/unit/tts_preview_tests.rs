use super::*;
use crate::app::voice_window_inset::TtsPlaybackEvent;
use audio::PcmFormat;

fn voiced_pcm() -> PcmAudioChunk {
    PcmAudioChunk::new(
        PcmFormat::Pcm16,
        16_000,
        1,
        8_000i16
            .to_le_bytes()
            .into_iter()
            .cycle()
            .take(3_200)
            .collect(),
    )
    .expect("100ms 的 16kHz mono PCM16 测试数据必须合法")
}

/// Voice Preview 仅由实际排队、开始与完成回调产生共享播放动画事件。
///
/// 数据构造（含关键数值的推导过程）：
///   PCM samples = 3_200 bytes / 2 bytes-per-sample = 1_600 samples
///   duration    = 1_600 / 16_000Hz = 100ms
///   callbacks   = queued + started + finished = 3 个事件
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 开始一轮 preview 并提交已被 sink 接受的 PCM → 只记录 PcmQueued。
///   2. 注入实际 playback-started 回调 → 记录 Started。
///   3. 注入完成回调 → 记录 Finished 并结束 active 状态。
///
/// 预期结果：
///   - 正断言：三个事件严格按 PcmQueued、Started、Finished 排列。
///   - 正断言：完成后 preview 不再 active。
///   - 负断言：正常完成路径不得生成 Interrupted。
#[test]
fn test_tts_preview_emits_actual_pcm_playback_events_in_order() {
    let mut preview = TtsPreviewState::default();
    let (generation, _control) = preview.start();

    preview.queue_pcm(generation, &voiced_pcm());
    preview.mark_playback_started(generation);
    preview.finish_playback(generation);
    let events = preview.take_playback_events();

    assert_eq!(
        events.len(),
        3,
        "100ms PCM 的正常 preview 应精确产生排队、开始、完成三个事件"
    );
    assert!(
        matches!(events.first(), Some(TtsPlaybackEvent::PcmQueued(_))),
        "第一个事件必须是实际 PCM 已被 sink 接受后的 PcmQueued"
    );
    assert!(
        matches!(events.get(1), Some(TtsPlaybackEvent::Started)),
        "第二个事件必须来自实际 playback-started 回调"
    );
    assert!(
        matches!(events.get(2), Some(TtsPlaybackEvent::Finished)),
        "第三个事件必须来自实际 playback completion 回调"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, TtsPlaybackEvent::Interrupted)),
        "正常完成的 Voice Preview 不得生成 Interrupted"
    );
    assert!(
        !preview.is_active(),
        "收到 preview 完成回调后 active 状态必须清除"
    );
}

/// Voice Preview 外部取消同步：Dialogue 停止 control 后，Preview 状态必须产生中断并释放占用。
///
/// 数据构造（含关键数值的推导过程）：
///   PCM duration = 3_200 bytes / 2 / 16_000Hz = 100ms
///   playback state = queued + started，共 1 个 active preview
///   external cancellation = 1 次 control.cancel()
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 preview、排入 PCM 并标记真实播放开始。
///   2. 模拟 Dialogue 优先级路径直接取消共享 control。
///   3. Preview 状态在帧同步点识别外部取消并输出 Interrupted。
///
/// 预期结果：
///   - 正断言：同步后存在 Interrupted，用于复用窗口内缘中断动画。
///   - 负断言：preview 不再 active，不能永久阻塞 Agent 队列。
///   - 负断言：取消路径不得错误产生 Finished。
#[test]
fn test_tts_preview_synchronizes_external_control_cancellation() {
    let mut preview = TtsPreviewState::default();
    let (generation, control) = preview.start();
    preview.queue_pcm(generation, &voiced_pcm());
    preview.mark_playback_started(generation);
    let _ = preview.take_playback_events();

    control.cancel();
    preview.synchronize_external_cancellation();
    let events = preview.take_playback_events();

    assert!(
        events
            .iter()
            .any(|event| matches!(event, TtsPlaybackEvent::Interrupted)),
        "外部取消已开始的 Preview 后必须生成 Interrupted 以收束共享动画"
    );
    assert!(
        !preview.is_active(),
        "外部取消同步后 Preview 必须释放 active 状态，不能阻塞 Agent 播报"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, TtsPlaybackEvent::Finished)),
        "被取消的 Preview 不得被错误标记为正常 Finished"
    );
}

/// Voice Preview 取消顺序：尚未被帧循环消费的正向事件必须在终止事件前移除。
///
/// 数据构造（含关键数值的推导过程）：
///   PCM duration = 3_200 bytes / 2 / 16_000Hz = 100ms
///   undrained positive events = PcmQueued + Started = 2 个
///   terminal events after cancel = Interrupted = 1 个
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Preview，排入 100ms PCM 并标记已开始，但不 drain 事件。
///   2. 在助手回复开始前取消 Preview。
///   3. 读取剩余事件，模拟下一动画帧按来源应用。
///
/// 预期结果：
///   - 正断言：只保留 Interrupted 以终止 Preview 时间线。
///   - 负断言：PcmQueued 与 Started 不得迟到后重新抢占助手时间线。
#[test]
fn test_tts_preview_cancel_removes_undrained_positive_events() {
    let mut preview = TtsPreviewState::default();
    let (generation, _control) = preview.start();
    preview.queue_pcm(generation, &voiced_pcm());
    preview.mark_playback_started(generation);

    preview.cancel();
    let events = preview.take_playback_events();

    assert_eq!(
        events.len(),
        1,
        "取消已开始 Preview 后只能保留一个 Interrupted 终止事件"
    );
    assert!(
        matches!(events.first(), Some(TtsPlaybackEvent::Interrupted)),
        "取消已开始 Preview 必须保留 Interrupted 以收束旧时间线"
    );
    assert!(
        !events.iter().any(|event| matches!(
            event,
            TtsPlaybackEvent::PcmQueued(_) | TtsPlaybackEvent::Started
        )),
        "未 drain 的 Preview PcmQueued/Started 不得在取消后重新抢占助手时间线"
    );
}
