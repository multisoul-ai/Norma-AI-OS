use super::playback::start_playback_with;
use audio::{PcmAudioChunk, PcmFormat};
use std::{sync::mpsc, time::Duration};

struct CompletionSink {
    completion: mpsc::Sender<async_channel::Sender<()>>,
}

impl super::super::voice::playback::AssistantAudioSink for CompletionSink {
    fn write_chunk(&mut self, chunk: &PcmAudioChunk) -> Result<Duration, String> {
        Ok(super::super::voice::playback::pcm_chunk_duration(chunk))
    }

    fn write_chunk_with_completion(
        &mut self,
        chunk: &PcmAudioChunk,
        playback_started: async_channel::Sender<()>,
        completion: async_channel::Sender<()>,
    ) -> Result<Duration, String> {
        let _ = playback_started.try_send(());
        self.completion
            .send(completion)
            .map_err(|_| "测试播放完成通道已关闭".to_string())?;
        self.write_chunk(chunk)
    }

    fn stop(&mut self) {}
}

/// DB-11：豆包 PCM 播放驱动白色窗口内缘光，硬件播完之前不得提前淡出。
///
/// 数据构造（含关键数值的推导过程）：24 kHz × 2 字节 × 20 ms = 960 字节 PCM；
///   一次完整播报只有这一段，模拟硬件完成回调由测试明确释放。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 播放线程调度 PCM → 产生能量与开始事件。
///   2. TTS 流结束但硬件回调未到 → 播报仍等待完成。
///   3. 触发硬件完成回调 → 播报结束并产生结束事件。
///
/// 预期结果：豆包开始时有能量与开始事件；回调之前没有结束事件；
///   回调之后有结束事件，且播放完成通知不再悬而未决。
#[test]
fn test_doubao_playback_drives_inset_until_audio_completion() {
    use crate::app::voice_window_inset::{
        TtsPlaybackEvent, TtsPlaybackSource, VoiceWindowInsetEventBus,
    };

    let (events, _wake) = VoiceWindowInsetEventBus::new();
    let (completion_tx, completion_rx) = mpsc::channel();
    let playback = start_playback_with(
        move || {
            Box::new(CompletionSink {
                completion: completion_tx,
            })
        },
        events.sink(TtsPlaybackSource::Doubao),
    );
    let pcm = PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0; 960])
        .expect("20 ms 豆包 PCM 应有效");
    let generation = playback.generation();
    playback
        .enqueue_for(generation, pcm)
        .expect("豆包 PCM 应进入播放器");
    let hardware_done = completion_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("播放器应提供硬件完成回调");
    let speech_done = playback
        .finish_for(generation)
        .expect("TTS 终态应能等待硬件完成");

    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut before = Vec::new();
    while !before
        .iter()
        .any(|(_, event)| matches!(event, TtsPlaybackEvent::Started))
        && std::time::Instant::now() < deadline
    {
        before.extend(events.drain());
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        before.iter().any(|(source, event)| matches!(
            (source, event),
            (TtsPlaybackSource::Doubao, TtsPlaybackEvent::PcmQueued(_))
        )),
        "豆包 PCM 必须把能量送入白色边框"
    );
    assert!(
        before.iter().any(|(source, event)| matches!(
            (source, event),
            (TtsPlaybackSource::Doubao, TtsPlaybackEvent::Started)
        )),
        "实际播放开始后白色边框必须激活"
    );
    assert!(
        !before
            .iter()
            .any(|(_, event)| matches!(event, TtsPlaybackEvent::Finished)),
        "硬件回调之前白色边框不得提前结束"
    );
    assert!(
        speech_done.try_recv().is_err(),
        "硬件尚未播完时豆包回复不得报告完成"
    );

    hardware_done.try_send(()).expect("应释放硬件完成回调");
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut finished = Vec::new();
    let completion_notified = loop {
        if speech_done.try_recv().is_ok() {
            break true;
        }
        if std::time::Instant::now() >= deadline {
            break false;
        }
        finished.extend(events.drain());
        std::thread::sleep(Duration::from_millis(2));
    };
    finished.extend(events.drain());
    assert!(
        completion_notified,
        "最后一段硬件回调后豆包回复必须报告完成"
    );
    assert!(
        finished.iter().any(|(source, event)| matches!(
            (source, event),
            (TtsPlaybackSource::Doubao, TtsPlaybackEvent::Finished)
        )),
        "最后一段硬件播放完成后白色边框必须收束"
    );
    playback.end();
}

/// DB-12：音频完成回调丢失时不得把未播完语音当成正常完成。
///
/// 数据构造（含关键数值的推导过程）：24 kHz × 2 字节 × 20 ms = 960 字节 PCM；
///   一段音频的完成回调发送端被丢弃，模拟播放后端没有报告 DataPlayedBack。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调度一段 PCM 并提交 TTS 终态。
///   2. 丢弃硬件完成回调 → 播放器检查到通道关闭。
///
/// 预期结果：明确报告播放错误并中断白光；不得发送 Finished 或成功完成通知。
#[test]
fn test_doubao_missing_audio_completion_reports_error() {
    use crate::app::voice_window_inset::{
        TtsPlaybackEvent, TtsPlaybackSource, VoiceWindowInsetEventBus,
    };

    let (events, _wake) = VoiceWindowInsetEventBus::new();
    let (completion_tx, completion_rx) = mpsc::channel();
    let playback = start_playback_with(
        move || {
            Box::new(CompletionSink {
                completion: completion_tx,
            })
        },
        events.sink(TtsPlaybackSource::Doubao),
    );
    let generation = playback.generation();
    let pcm = PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0; 960])
        .expect("20 ms 豆包 PCM 应有效");
    playback
        .enqueue_for(generation, pcm)
        .expect("豆包 PCM 应进入播放器");
    let hardware_done = completion_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("播放器应提供硬件完成回调");
    let speech_done = playback
        .finish_for(generation)
        .expect("TTS 终态应等待硬件完成");
    drop(hardware_done);

    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let error = loop {
        if let Ok(error) = playback.errors.try_recv() {
            break error;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "完成回调丢失必须在 2 秒内转成播放错误"
        );
        std::thread::sleep(Duration::from_millis(2));
    };
    assert_eq!(
        error.resolve_for(ui::i18n::Locale::ZhCn),
        "豆包音频播放完成回调丢失",
        "后端无完成回调不得被当作已播完"
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut visual_events = Vec::new();
    while !visual_events
        .iter()
        .any(|(_, event)| matches!(event, TtsPlaybackEvent::Interrupted))
        && std::time::Instant::now() < deadline
    {
        visual_events.extend(events.drain());
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        visual_events
            .iter()
            .any(|(_, event)| matches!(event, TtsPlaybackEvent::Interrupted)),
        "播放失败后白色边框必须进入中断淡出"
    );
    assert!(
        !visual_events
            .iter()
            .any(|(_, event)| matches!(event, TtsPlaybackEvent::Finished)),
        "完成回调丢失时白色边框不得正常结束"
    );
    assert!(
        speech_done.try_recv().is_err(),
        "完成回调丢失时豆包回复不得报告成功"
    );
}

/// DB-13：播放后端一直持有回调却不发送完成信号时，豆包播报必须超时退出。
///
/// 数据构造（含关键数值的推导过程）：24,000 帧/秒 × 2 字节 × 20 ms = 960 字节；
///   播放预计 20 ms，允许 5 秒设备回调余量，测试最多等待 7 秒。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调度一段 PCM 并持有其完成回调发送端，但始终不发送。
///   2. 请求本轮播放完成，等待预计播放时长与回调余量过去。
///   3. 播放线程报告超时并结束等待。
///
/// 预期结果：收到明确的播放超时错误和中断事件；不得收到正常完成通知。
#[test]
fn test_doubao_stalled_audio_completion_times_out() {
    use crate::app::voice_window_inset::{
        TtsPlaybackEvent, TtsPlaybackSource, VoiceWindowInsetEventBus,
    };

    let (events, _wake) = VoiceWindowInsetEventBus::new();
    let (completion_tx, completion_rx) = mpsc::channel();
    let playback = start_playback_with(
        move || {
            Box::new(CompletionSink {
                completion: completion_tx,
            })
        },
        events.sink(TtsPlaybackSource::Doubao),
    );
    let generation = playback.generation();
    let pcm = PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0; 960])
        .expect("20 ms 豆包 PCM 应有效");
    playback.enqueue_for(generation, pcm).expect("PCM 应入队");
    let _held_callback = completion_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("播放器应持有完成回调");
    let speech_done = playback.finish_for(generation).expect("应能等待播放完成");
    let deadline = std::time::Instant::now() + Duration::from_secs(7);
    let error = loop {
        if let Ok(error) = playback.errors.try_recv() {
            break error;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "完成回调一直悬挂时，播放线程必须在 7 秒内超时"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(
        error.resolve_for(ui::i18n::Locale::ZhCn),
        "豆包音频播放完成回调超时",
        "应明确报告硬件回调超时"
    );
    let visual_deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut visual_events = Vec::new();
    while !visual_events
        .iter()
        .any(|(_, event)| matches!(event, TtsPlaybackEvent::Interrupted))
        && std::time::Instant::now() < visual_deadline
    {
        visual_events.extend(events.drain());
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        visual_events
            .iter()
            .any(|(_, event)| matches!(event, TtsPlaybackEvent::Interrupted)),
        "播放超时后白色边框应中断"
    );
    assert!(
        !visual_events
            .iter()
            .any(|(_, event)| matches!(event, TtsPlaybackEvent::Finished)),
        "播放超时不得产生正常完成事件"
    );
    assert!(speech_done.try_recv().is_err(), "播放超时不得报告正常完成");
}

/// DB-14：碎片 PCM 不得让播放器无限积累待完成回调。
///
/// 数据构造（含关键数值的推导过程）：每段 24,000 × 2 × 0.02 = 960 字节；
///   512 段为 10,240 ms 排队音频，第 513 段应等一段完成才调度。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 逐段入队并持有前 512 个回调，使它们保持待完成。
///   2. 入队第 513 段，确认播放器暂不继续调度。
///   3. 完成第一段，确认第 513 段随后得到调度。
///
/// 预期结果：待完成回调有上限；释放名额后继续播放，不丢弃后续音频。
#[test]
fn test_doubao_fragmented_audio_limits_outstanding_callbacks() {
    use crate::app::voice_window_inset::{TtsPlaybackSource, VoiceWindowInsetEventBus};

    let (events, _wake) = VoiceWindowInsetEventBus::new();
    let (completion_tx, completion_rx) = mpsc::channel();
    let playback = start_playback_with(
        move || {
            Box::new(CompletionSink {
                completion: completion_tx,
            })
        },
        events.sink(TtsPlaybackSource::Doubao),
    );
    let generation = playback.generation();
    let mut held_callbacks = Vec::new();
    for _ in 0..512 {
        let pcm = PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0; 960])
            .expect("20 ms 豆包 PCM 应有效");
        playback.enqueue_for(generation, pcm).expect("PCM 应入队");
        held_callbacks.push(
            completion_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("前 512 段应得到调度"),
        );
    }
    let next_pcm = PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0; 960])
        .expect("第 513 段 PCM 应有效");
    playback
        .enqueue_for(generation, next_pcm)
        .expect("第 513 段应入队");
    assert!(
        completion_rx
            .recv_timeout(Duration::from_millis(50))
            .is_err(),
        "前 512 个硬件回调未完成时，不应继续调度第 513 段"
    );
    held_callbacks[0].try_send(()).expect("第一段回调应能完成");
    assert!(
        completion_rx.recv_timeout(Duration::from_secs(2)).is_ok(),
        "第一段完成后应调度第 513 段"
    );
    playback.end();
}
