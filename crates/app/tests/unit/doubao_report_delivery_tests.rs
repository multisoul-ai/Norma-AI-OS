use super::*;
use crate::app::voice::playback::AssistantAudioSink;
use crate::app::voice_window_inset::{TtsPlaybackSource, VoiceWindowInsetEventBus};
use audio::PcmFormat;
use std::sync::mpsc;

type Callbacks = (async_channel::Sender<()>, async_channel::Sender<()>);
struct ControlledSink(mpsc::Sender<Callbacks>);
impl AssistantAudioSink for ControlledSink {
    fn write_chunk(&mut self, chunk: &PcmAudioChunk) -> Result<Duration, String> {
        Ok(crate::app::voice::playback::pcm_chunk_duration(chunk))
    }
    fn write_chunk_with_completion(
        &mut self,
        chunk: &PcmAudioChunk,
        started: async_channel::Sender<()>,
        completed: async_channel::Sender<()>,
    ) -> Result<Duration, String> {
        self.0
            .send((started, completed))
            .map_err(|e| e.to_string())?;
        self.write_chunk(chunk)
    }
    fn stop(&mut self) {}
}

/// 场景：豆包音频已调度，但硬件尚未实际开始播放。
/// 构造：两段 24kHz × 2B × 20ms = 960B PCM；开始及完成回调都由夹具保留。
/// 执行：收到第二段调度回调后检查事件（保证首段调度已结束），再依次释放开始、完成。
/// 预期：提交 PCM 不能等于 Started；真实开始后才 Started，真实完成后才 Finished。
#[test]
fn doubao_report_delivery_waits_for_hardware_start_and_completion() {
    let (_dir, service, reference, delivery) = saved_report();
    let (events, _wake) = VoiceWindowInsetEventBus::new();
    let (send, callbacks) = mpsc::channel();
    let playback = start_playback_with(
        move || Box::new(ControlledSink(send)),
        events.sink(TtsPlaybackSource::Doubao),
    );
    let generation = playback.generation();
    playback.track_report(generation, delivery.clone()).unwrap();
    let pcm = PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0; 960]).unwrap();
    playback.enqueue_for(generation, pcm.clone()).unwrap();
    playback.enqueue_for(generation, pcm).unwrap();
    let (started1, completed1) = callbacks.recv_timeout(Duration::from_secs(2)).unwrap();
    let (started2, completed2) = callbacks.recv_timeout(Duration::from_secs(2)).unwrap();
    let before = events.drain();
    assert!(
        before
            .iter()
            .any(|(_, e)| matches!(e, TtsPlaybackEvent::PcmQueued(_))),
        "首段应已调度"
    );
    assert!(
        !before
            .iter()
            .any(|(_, e)| matches!(e, TtsPlaybackEvent::Started)),
        "硬件开始回调尚未释放，不能把调度当作开播"
    );
    assert_status(&service, &reference, &delivery, ReportDelivery::Sent);
    started1.try_send(()).unwrap();
    started2.try_send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let events = events.drain();
        assert!(
            !events
                .iter()
                .any(|(_, e)| matches!(e, TtsPlaybackEvent::Finished)),
            "完成回调之前不得宣布播完"
        );
        if events
            .iter()
            .any(|(_, e)| matches!(e, TtsPlaybackEvent::Started))
        {
            break;
        }
        assert!(Instant::now() < deadline, "真实开始必须被观察到");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_status(&service, &reference, &delivery, ReportDelivery::Playing);
    let done = playback.finish_for(generation).unwrap();
    assert!(done.try_recv().is_err(), "TTS 结束不等于硬件完成");
    completed1.try_send(()).unwrap();
    completed2.try_send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if done.try_recv().is_ok() {
            break;
        }
        assert!(Instant::now() < deadline, "硬件完成后应结束当前播报");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        events
            .drain()
            .iter()
            .any(|(_, e)| matches!(e, TtsPlaybackEvent::Finished)),
        "真实完成应产生 Finished"
    );
    assert_status(&service, &reference, &delivery, ReportDelivery::Completed);
    playback.end();
    crate::app::shared_tokio_runtime()
        .unwrap()
        .block_on(playback.wait_stopped());
}

fn saved_report() -> (
    tempfile::TempDir,
    crate::app::shared_context::SharedContext,
    agent::context::ContextRef,
    ReportDeliveryHandle,
) {
    let dir = tempfile::tempdir().unwrap();
    let service = crate::app::shared_context::SharedContext::new(
        dir.path().join("app.db"),
        agent::context::ContextScope {
            workspace_id: "w".into(),
            area_id: "a".into(),
            project: agent::context::ContextProject::Personal,
            task_id: None,
            conversation_id: None,
        },
    )
    .unwrap();
    let reference = crate::app::shared_tokio_runtime()
        .unwrap()
        .block_on(service.run(None, |access| access.report("hardware", "报告", vec![])))
        .unwrap()
        .reference;
    let delivery = service.report_delivery(reference.clone()).unwrap();
    delivery.set(ReportDelivery::Queued);
    delivery.set(ReportDelivery::Sent);
    (dir, service, reference, delivery)
}

fn assert_status(
    service: &crate::app::shared_context::SharedContext,
    reference: &agent::context::ContextRef,
    delivery: &ReportDeliveryHandle,
    expected: ReportDelivery,
) {
    let runtime = crate::app::shared_tokio_runtime().unwrap();
    runtime.block_on(delivery.flush()).unwrap();
    let snapshot = runtime.block_on(service.snapshot()).unwrap();
    assert_eq!(
        snapshot.state.reports[&reference.event_id].delivery, expected,
        "共享记录必须对应实际音频回调，而非网络或调度状态"
    );
}

struct SchedulingBarrier {
    callbacks: mpsc::Sender<Callbacks>,
    release: mpsc::Receiver<()>,
}
impl AssistantAudioSink for SchedulingBarrier {
    fn write_chunk(&mut self, _: &PcmAudioChunk) -> Result<Duration, String> {
        unreachable!("夹具只接受带硬件回调的调度")
    }
    fn write_chunk_with_completion(
        &mut self,
        pcm: &PcmAudioChunk,
        started: async_channel::Sender<()>,
        completed: async_channel::Sender<()>,
    ) -> Result<Duration, String> {
        self.callbacks.send((started, completed)).unwrap();
        self.release.recv_timeout(Duration::from_secs(2)).unwrap();
        Ok(crate::app::voice::playback::pcm_chunk_duration(pcm))
    }
    fn stop(&mut self) {}
}

/// 场景：硬件开播与用户插话发生在 worker 尚未处理开始回调的同一窗口。
/// 构造：调度屏障暂停 worker；24kHz × 2B × 20ms = 960B PCM、真实持久化报告。
/// 执行：分别在无开始／有开始回调时中断，随后释放调度并结束 worker，再尝试迟到完成。
/// 预期：未播为 Skipped，已播为 Interrupted；迟到完成不能写成 Completed 或覆盖终态。
#[test]
fn doubao_report_delivery_interrupt_observes_pending_hardware_start() {
    for started in [false, true] {
        let (_dir, service, reference, delivery) = saved_report();
        let (events, _wake) = VoiceWindowInsetEventBus::new();
        let (send, callbacks) = mpsc::channel();
        let (release, barrier) = mpsc::channel();
        let playback = start_playback_with(
            move || {
                Box::new(SchedulingBarrier {
                    callbacks: send,
                    release: barrier,
                })
            },
            events.sink(TtsPlaybackSource::Doubao),
        );
        let generation = playback.generation();
        playback.track_report(generation, delivery.clone()).unwrap();
        playback
            .enqueue_for(
                generation,
                PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0; 960]).unwrap(),
            )
            .unwrap();
        let (start, complete) = callbacks.recv_timeout(Duration::from_secs(2)).unwrap();
        if started {
            start.try_send(()).unwrap();
        }
        playback.interrupt();
        release.send(()).unwrap();
        playback.end();
        crate::app::shared_tokio_runtime()
            .unwrap()
            .block_on(playback.wait_stopped());
        let _ = complete.try_send(());
        assert_status(
            &service,
            &reference,
            &delivery,
            if started {
                ReportDelivery::Interrupted
            } else {
                ReportDelivery::Skipped
            },
        );
        assert!(
            !events
                .drain()
                .iter()
                .any(|(_, e)| matches!(e, TtsPlaybackEvent::Finished)),
            "取消后不得声称已播完"
        );
    }
}

struct StartBeforeStop {
    scheduled: mpsc::Sender<()>,
    started: Option<async_channel::Sender<()>>,
    completed: Option<async_channel::Sender<()>>,
}
impl AssistantAudioSink for StartBeforeStop {
    fn write_chunk(&mut self, _: &PcmAudioChunk) -> Result<Duration, String> {
        unreachable!("需要硬件回调")
    }
    fn write_chunk_with_completion(
        &mut self,
        pcm: &PcmAudioChunk,
        started: async_channel::Sender<()>,
        completed: async_channel::Sender<()>,
    ) -> Result<Duration, String> {
        self.started = Some(started);
        self.completed = Some(completed);
        self.scheduled.send(()).unwrap();
        Ok(crate::app::voice::playback::pcm_chunk_duration(pcm))
    }
    fn stop(&mut self) {
        // A hardware start races shutdown, before stop has actually completed.
        if let Some(started) = self.started.take() {
            let _ = started.try_send(());
        }
    }
}

/// 场景：上游提前退出，直接释放最后一个队列，硬件在 stop 确认前开始播放。
/// 构造：已保存报告、一段 20ms PCM，sink 在停止返回前发送真实开始信号。
/// 执行：等待调度后直接 drop 队列（不调用 end），等待 worker 退出并读取共享状态。
/// 预期：清理阶段仍记录 Interrupted，不将已开播报告误写 Skipped。
#[test]
fn doubao_report_delivery_drop_observes_start_before_stop_confirmation() {
    let (_dir, service, reference, delivery) = saved_report();
    let (events, _wake) = VoiceWindowInsetEventBus::new();
    let (scheduled, received) = mpsc::channel();
    let playback = start_playback_with(
        move || {
            Box::new(StartBeforeStop {
                scheduled,
                started: None,
                completed: None,
            })
        },
        events.sink(TtsPlaybackSource::Doubao),
    );
    playback.track_report(0, delivery.clone()).unwrap();
    playback
        .enqueue_for(
            0,
            PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0; 960]).unwrap(),
        )
        .unwrap();
    received.recv_timeout(Duration::from_secs(2)).unwrap();
    let stopped = playback.stopped.clone();
    drop(playback);
    crate::app::shared_tokio_runtime().unwrap().block_on(async {
        let _ = tokio::time::timeout(Duration::from_secs(2), stopped.recv())
            .await
            .unwrap();
    });
    assert_status(&service, &reference, &delivery, ReportDelivery::Interrupted);
}
