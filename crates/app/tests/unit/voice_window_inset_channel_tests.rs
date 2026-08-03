use super::*;

/// VWI-7：同一显示帧前到达的多个 TTS 事件必须保序，但只产生一个合并唤醒。
///
/// 数据构造（含关键数值的推导过程）：
///   wake channel capacity = 1
///   queued semantic events = Started + Finished + Interrupted = 3
///   expected wake signals = min(3 events, channel capacity 1) = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 为 Assistant 来源创建事件 sink，连续写入三个有序语义事件。
///   2. 在消费任何事件前读取 wake channel 两次。
///   3. 一次性 drain 语义队列，并再次读取活动状态。
///
/// 预期结果：
///   - 正断言：第一次读取获得唯一 wake，三个语义事件仍按原顺序完整存在。
///   - 负断言：第二次读取不得再获得 wake，证明同一帧前的通知已合并。
///   - 正断言：drain 前 pending 令总线活动，drain 后总线不再误报 pending。
#[test]
fn voice_window_inset_event_bus_coalesces_wake_without_dropping_ordered_events() {
    let (events, wake_rx) = VoiceWindowInsetEventBus::new();
    let assistant = events.sink(TtsPlaybackSource::Assistant);

    assistant.push(TtsPlaybackEvent::Started);
    assistant.push(TtsPlaybackEvent::Finished);
    assistant.push(TtsPlaybackEvent::Interrupted);

    assert!(
        events.is_animation_active(),
        "三个待消费语义事件存在时事件总线必须报告活动"
    );
    assert!(
        wake_rx.try_recv().is_ok(),
        "capacity=1 的 wake channel 必须保留第一个合并唤醒"
    );
    assert!(
        matches!(wake_rx.try_recv(), Err(async_channel::TryRecvError::Empty)),
        "连续三个事件不得产生第二个 wake；否则 PCM 分片仍会放大窗口刷新"
    );

    let drained = events.drain();
    assert_eq!(
        drained.len(),
        3,
        "合并 wake 不得丢失三个语义事件中的任何一个"
    );
    assert!(
        matches!(
            drained.first(),
            Some((TtsPlaybackSource::Assistant, TtsPlaybackEvent::Started))
        ),
        "drain 后第一个事件必须仍是 Assistant Started"
    );
    assert!(
        matches!(
            drained.get(1),
            Some((TtsPlaybackSource::Assistant, TtsPlaybackEvent::Finished))
        ),
        "drain 后第二个事件必须仍是 Assistant Finished"
    );
    assert!(
        matches!(
            drained.get(2),
            Some((TtsPlaybackSource::Assistant, TtsPlaybackEvent::Interrupted))
        ),
        "drain 后第三个事件必须仍是 Assistant Interrupted"
    );
    assert!(
        !events.is_animation_active(),
        "语义队列 drain 完且 View 尚未激活动画时总线不得残留 pending 活动"
    );
}
