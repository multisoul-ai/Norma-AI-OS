use super::*;
use crate::app::voice::playback::AssistantAudioSink;
use crate::app::voice_window_inset::{TtsPlaybackSource, VoiceWindowInsetEventBus};

struct HeldSink {
    scheduled: crossbeam_channel::Sender<async_channel::Sender<()>>,
    stopping: crossbeam_channel::Sender<()>,
    stopped: crossbeam_channel::Receiver<()>,
}
impl AssistantAudioSink for HeldSink {
    fn write_chunk(&mut self, _: &PcmAudioChunk) -> Result<Duration, String> {
        unreachable!("test requires real completion notifications")
    }
    fn write_chunk_with_completion(
        &mut self,
        pcm: &PcmAudioChunk,
        _: async_channel::Sender<()>,
        done: async_channel::Sender<()>,
    ) -> Result<Duration, String> {
        self.scheduled.send(done).expect("schedule observer");
        Ok(crate::app::voice::playback::pcm_chunk_duration(pcm))
    }
    fn stop(&mut self) {
        let _ = self.stopping.send(());
        let _ = self.stopped.recv();
    }
}
fn pcm_600ms() -> PcmAudioChunk {
    // 16000 samples/s × 0.6s × 2B = 19200B, two chunks exceed the 1s budget.
    PcmAudioChunk::new(audio::PcmFormat::Pcm16, 16000, 1, vec![0; 19200]).expect("600ms PCM")
}

/// AC03/AC11：Live的1秒播放预算覆盖排队和硬件已schedule但未完成的PCM。
/// 数据构造：600ms PCM、持有完成回调及可阻塞stop的sink；预算上限1000ms。
/// 执行过程：schedule首包→拒绝第二包→真实完成首包→接受新包→阻塞interrupt/stop→拒绝超限→释放stop。
/// 预期结果：墙钟和出队不能释放预算；只有完成/实际stop释放；关闭后预算归零且不能继续schedule。
#[tokio::test]
async fn live_playback_budget_covers_hardware_until_completion_or_stop() {
    let (scheduled, scheduled_rx) = crossbeam_channel::unbounded();
    let (stopping, stopping_rx) = crossbeam_channel::unbounded();
    let (stopped_tx, stopped) = crossbeam_channel::unbounded();
    let (bus, _wake) = VoiceWindowInsetEventBus::new();
    let playback = start_live_playback_with(
        move || {
            Box::new(HeldSink {
                scheduled,
                stopping,
                stopped,
            })
        },
        bus.sink(TtsPlaybackSource::GptLive),
    );
    playback.enqueue_for(0, pcm_600ms()).expect("首包在预算内");
    let first = scheduled_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("首包确实schedule");
    assert!(
        playback.enqueue_for(0, pcm_600ms()).is_err(),
        "已schedule未完成的600ms也占预算，第二包必须拒绝"
    );
    assert_eq!(
        playback.buffered_duration(),
        Duration::from_millis(600),
        "预算包括硬件积压"
    );
    first.try_send(()).expect("实际完成首包");
    let deadline = Instant::now() + Duration::from_secs(2);
    while playback.buffered_duration() != Duration::ZERO && Instant::now() < deadline {
        tokio::task::yield_now().await;
    }
    assert_eq!(
        playback.buffered_duration(),
        Duration::ZERO,
        "完成回调释放预算"
    );
    playback.enqueue_for(0, pcm_600ms()).expect("完成后可继续");
    let _held = scheduled_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("新包schedule");
    playback.interrupt();
    stopping_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("进入stop但未返回");
    assert!(
        playback
            .enqueue_for(playback.generation(), pcm_600ms())
            .is_err(),
        "stop实际返回前旧硬件仍占预算"
    );
    stopped_tx.send(()).expect("允许stop完成");
    let deadline = Instant::now() + Duration::from_secs(2);
    while playback.buffered_duration() != Duration::ZERO && Instant::now() < deadline {
        tokio::task::yield_now().await;
    }
    assert_eq!(
        playback.buffered_duration(),
        Duration::ZERO,
        "stop后预算释放"
    );
    playback
        .enqueue_for(playback.generation(), pcm_600ms())
        .expect("中断后新代可继续");
    let _last = scheduled_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("新代schedule");
    playback.end();
    stopped_tx.send(()).expect("interrupt stop返回");
    stopped_tx.send(()).expect("shutdown stop返回");
    tokio::time::timeout(Duration::from_secs(2), playback.wait_stopped())
        .await
        .expect("退出有界");
    assert_eq!(
        playback.buffered_duration(),
        Duration::ZERO,
        "退出释放所有预算"
    );
    assert!(
        playback
            .enqueue_for(playback.generation(), pcm_600ms())
            .is_err(),
        "退出后拒绝音频"
    );
    assert!(scheduled_rx.try_recv().is_err(), "拒绝的音频不得schedule");
}

/// AC03/AC11：丢失硬件完成回调不等于硬件已停止，不得提前释放时长预算。
/// 数据构造：600ms已schedule后主动丢弃完成Sender，sink的stop等待显式许可。
/// 执行过程：触发callback lost→等待stop进入→读取预算并尝试第二个600ms→允许stop/退出。
/// 预期结果：异常清理期间旧600ms仍占预算，第二包被拒绝，退出后才归零。
#[tokio::test]
async fn live_playback_lost_callback_holds_budget_until_confirmed_stop() {
    let (scheduled, scheduled_rx) = crossbeam_channel::unbounded();
    let (stopping, stopping_rx) = crossbeam_channel::unbounded();
    let (stopped_tx, stopped) = crossbeam_channel::unbounded();
    let (bus, _wake) = VoiceWindowInsetEventBus::new();
    let playback = start_live_playback_with(
        move || {
            Box::new(HeldSink {
                scheduled,
                stopping,
                stopped,
            })
        },
        bus.sink(TtsPlaybackSource::GptLive),
    );
    playback.enqueue_for(0, pcm_600ms()).expect("首包");
    let callback = scheduled_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("已schedule");
    drop(callback);
    stopping_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("丢失回调后停止硬件");
    let held = playback.buffered_duration();
    let overflow_rejected = playback.enqueue_for(0, pcm_600ms()).is_err();
    stopped_tx.send(()).expect("允许硬件停止");
    tokio::time::timeout(Duration::from_secs(2), playback.wait_stopped())
        .await
        .expect("停止后退出");
    assert_eq!(
        held,
        Duration::from_millis(600),
        "callback丢失不能在硬件停止前释放预算"
    );
    assert!(overflow_rejected, "异常stop未确认前必须拒绝超限音频");
    assert_eq!(
        playback.buffered_duration(),
        Duration::ZERO,
        "退出后释放所有队列预算"
    );
    assert!(scheduled_rx.try_recv().is_err(), "异常后不schedule新音频");
}
