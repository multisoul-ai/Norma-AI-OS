use super::reply_stream::{QueuedPlaybackChunk, StreamPlaybackBuffer};
use super::{DialogueControlState, TtsPreviewControl};
use audio::{PcmAudioChunk, PcmFormat};
use std::sync::Arc;
use std::time::Duration;

fn pcm16_mono_chunk(sample_rate_hz: u32, duration: Duration) -> PcmAudioChunk {
    let frame_count = (duration.as_secs_f64() * f64::from(sample_rate_hz)) as usize;
    PcmAudioChunk::new(
        PcmFormat::Pcm16,
        sample_rate_hz,
        1,
        vec![0; frame_count * 2],
    )
    .expect("测试 PCM 应满足 PCM16 mono 格式约束")
}

/// STR-1：流式 TTS 必须在累计至少 120ms 后才启动第一块播放。
///
/// 数据构造：22.05kHz mono 的 60ms、60ms 两个 PCM 块；二者刚好累计为 120ms。
/// 执行过程：按顺序 push 两个块，并读取每次允许调度的输出。
/// 预期结果：第一块独自到达时不允许播放；第二块到达后两个块必须按原顺序一起释放。
#[test]
fn test_stream_playback_buffer_waits_for_120ms_then_releases_in_order() {
    let mut buffer = StreamPlaybackBuffer::new(Duration::from_millis(120));
    let first = pcm16_mono_chunk(22_050, Duration::from_millis(60));
    let second = pcm16_mono_chunk(22_050, Duration::from_millis(60));

    let before_threshold = buffer.push(QueuedPlaybackChunk::from_pcm(first.clone()));
    let at_threshold = buffer.push(QueuedPlaybackChunk::from_pcm(second.clone()));

    assert!(before_threshold.is_empty(), "累计 60ms 时不得提前启动播放");
    assert_eq!(
        at_threshold
            .into_iter()
            .map(|chunk| chunk.pcm)
            .collect::<Vec<_>>(),
        vec![first, second],
        "累计恰好 120ms 后必须按输入顺序释放所有预缓冲块"
    );
    assert!(buffer.is_empty(), "释放后预缓冲队列必须清空，避免重复播放");
}

/// STR-2：模型结束时不足 120ms 的尾部音频必须立即播放，不能靠插入静音凑门槛。
///
/// 数据构造：16kHz mono 的单个 80ms PCM 块，小于 120ms 启播门槛。
/// 执行过程：push 后模拟该 segment 的 TTS worker Finished，并调用 finish。
/// 预期结果：push 阶段仍不播放；finish 必须原样释放 80ms；不得留下待播块。
#[test]
fn test_stream_playback_buffer_flushes_short_final_audio_without_fake_silence() {
    let mut buffer = StreamPlaybackBuffer::new(Duration::from_millis(120));
    let short_final = pcm16_mono_chunk(16_000, Duration::from_millis(80));

    let before_finish = buffer.push(QueuedPlaybackChunk::from_pcm(short_final.clone()));
    let after_finish = buffer.finish();

    assert!(
        before_finish.is_empty(),
        "未结束时 80ms 不得突破 120ms 预缓冲门槛"
    );
    assert_eq!(
        after_finish
            .into_iter()
            .map(|chunk| chunk.pcm)
            .collect::<Vec<_>>(),
        vec![short_final],
        "最终短音频必须原样播放，不能补静音或被丢弃"
    );
    assert!(
        buffer.is_empty(),
        "最终 flush 后不得残留 PCM，避免下一段串音"
    );
}

/// STR-TTS-1：模型或 Voice 配置变更必须失效旧代际并取消正在进行的试听。
///
/// 数据构造（含关键数值的推导过程）：
///   initial configuration generation = 0（新建 DialogueControlState）
///   preview controls                 = 1（唯一的独立试听 sink 控制器）
///   queued speech segments            = 1（模拟旧 Voice 正在等待合成）
///   configuration changes            = 1（一次模型/Voice 变更）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 把一个未取消的试听控制器注册到 DialogueControlState；
///   2. 排入一段旧语音并标记合成活跃，再记录变更前的两个 generation；
///   3. 调用统一的配置变更取消路径并读取代际、队列与试听控制器状态。
///
/// 预期结果：
///   - 断言 A：generation 必须恰好增加 1，使旧异步结果失效；
///   - 断言 B：已登记的试听必须被取消，不能与后续回复混音；
///   - 断言 C：新 generation 不得等于旧 generation。
///   - 断言 D：旧合成队列与活跃标记必须清空，不能在新 Voice 下继续出音。
#[test]
fn test_tts_configuration_change_invalidates_generation_and_preview() {
    let mut dialogue = DialogueControlState::default();
    let control = Arc::new(TtsPreviewControl::new());
    dialogue.track_tts_preview(Arc::clone(&control));
    dialogue
        .speech_queue
        .push_back("old voice segment".to_string());
    dialogue.speech_synthesis_active = true;
    let configuration_before = dialogue.tts_configuration_generation();
    let reply_before = dialogue.reply_generation;

    dialogue.cancel_tts_for_configuration_change();

    assert_eq!(
        dialogue.tts_configuration_generation(),
        configuration_before.wrapping_add(1),
        "一次模型或 Voice 配置变更必须只推进一次 TTS configuration generation"
    );
    assert!(
        control.is_cancelled(),
        "配置变更必须取消已登记的 Voice preview，避免与后续助手回复混音"
    );
    assert_ne!(
        dialogue.tts_configuration_generation(),
        configuration_before,
        "配置变更后的 generation 不得仍等于旧值，否则旧异步事件可能写回"
    );
    assert_eq!(
        dialogue.reply_generation,
        reply_before.wrapping_add(1),
        "配置变更必须推进 reply generation，使旧 synthesis/playback 事件被丢弃"
    );
    assert!(
        dialogue.speech_queue.is_empty(),
        "配置变更必须清空旧 Voice 的待合成片段，不能在新 Voice 下继续播报"
    );
    assert!(
        !dialogue.speech_synthesis_active,
        "配置变更必须清除旧 synthesis 活跃标记，避免旧 worker 继续占用播放路径"
    );
}
