use super::reply_stream::{QueuedPlaybackChunk, StreamPlaybackBuffer};
use audio::{PcmAudioChunk, PcmFormat};
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
