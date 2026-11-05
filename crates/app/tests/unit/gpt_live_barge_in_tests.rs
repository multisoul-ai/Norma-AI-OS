use super::*;
use crate::app::doubao::playback::start_live_playback_with;
use crate::app::voice::playback::AssistantAudioSink;
use crate::app::voice_window_inset::{TtsPlaybackSource, VoiceWindowInsetEventBus};
use audio::PcmAudioChunk;
use std::time::{Duration, Instant};

/// Holds every hardware completion so scheduled PCM keeps occupying the budget.
struct HeldSink(crossbeam_channel::Sender<async_channel::Sender<()>>);
impl AssistantAudioSink for HeldSink {
    fn write_chunk(&mut self, _: &PcmAudioChunk) -> Result<Duration, String> {
        unreachable!("Live playback requires completion notifications")
    }
    fn write_chunk_with_completion(
        &mut self,
        pcm: &PcmAudioChunk,
        _: async_channel::Sender<()>,
        done: async_channel::Sender<()>,
    ) -> Result<Duration, String> {
        self.0.send(done).expect("schedule observer");
        Ok(crate::app::voice::playback::pcm_chunk_duration(pcm))
    }
    fn stop(&mut self) {}
}

fn pcm_ms(ms: usize) -> PcmAudioChunk {
    // 16000 samples/s × 2B = 32B per ms.
    PcmAudioChunk::new(audio::PcmFormat::Pcm16, 16000, 1, vec![0; ms * 32]).expect("PCM")
}

async fn wait_buffered(playback: &PlaybackQueue, expected: Duration) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while playback.buffered_duration() != expected && Instant::now() < deadline {
        tokio::task::yield_now().await;
    }
}

/// 风险修复：Live 无服务端清空命令；用户持续插话≥600ms 且本地积压超过1秒时丢弃旧语音。
/// 数据构造：硬件不回完成的sink；每段上传PCM 200ms；积压先800ms，再追加1s+1s+200ms到3s。
/// 执行过程：短“嗯”(400ms后静音)→持续说话但积压≤1s→同一句话积压到3s→新一句持续说话→丢弃后入队新音频。
/// 预期结果：短附和、积压≤1s都不丢；每句话最多处理一次；持续插话丢弃全部3s并报告；新音频正常计入预算。
#[tokio::test]
async fn barge_in_drops_stale_backlog_only_on_sustained_speech() {
    let (scheduled, held) = crossbeam_channel::unbounded();
    let (bus, _wake) = VoiceWindowInsetEventBus::new();
    let playback = start_live_playback_with(
        move || Box::new(HeldSink(scheduled)),
        bus.sink(TtsPlaybackSource::GptLive),
    );
    let mic = pcm_ms(200);
    let mut barge_in = BargeIn::default();
    playback.enqueue_for(0, pcm_ms(800)).expect("800ms");
    wait_buffered(&playback, Duration::from_millis(800)).await;
    for _ in 0..2 {
        assert_eq!(
            barge_in.observe(true, &mic, &playback),
            None,
            "400ms短附和不丢"
        );
    }
    assert_eq!(
        barge_in.observe(false, &mic, &playback),
        None,
        "说话结束重置"
    );
    for _ in 0..3 {
        assert_eq!(
            barge_in.observe(true, &mic, &playback),
            None,
            "积压≤1s时持续说话也不丢"
        );
    }
    for ms in [1000, 1000, 200] {
        playback
            .enqueue_for(0, pcm_ms(ms))
            .expect("单包≤1s，积压到3s");
    }
    wait_buffered(&playback, Duration::from_secs(3)).await;
    assert_eq!(
        barge_in.observe(true, &mic, &playback),
        None,
        "同一句话只判定一次"
    );
    assert_eq!(barge_in.observe(false, &mic, &playback), None, "说话结束");
    for _ in 0..2 {
        assert_eq!(barge_in.observe(true, &mic, &playback), None, "未满600ms");
    }
    assert_eq!(
        barge_in.observe(true, &mic, &playback),
        Some(Duration::from_secs(3)),
        "持续插话丢弃全部积压并报告时长"
    );
    wait_buffered(&playback, Duration::ZERO).await;
    assert_eq!(
        playback.buffered_duration(),
        Duration::ZERO,
        "stop后旧语音预算释放"
    );
    playback
        .enqueue_for(playback.generation(), pcm_ms(200))
        .expect("插话后新语音入队");
    wait_buffered(&playback, Duration::from_millis(200)).await;
    assert_eq!(
        playback.buffered_duration(),
        Duration::from_millis(200),
        "新代音频确实进入播放，而不是被代号过滤"
    );
    drop(held);
    playback.end();
}
