use super::*;
use crate::app::voice::playback::AssistantAudioSink;
use agent::context::ReportDelivery;
use audio::{PcmAudioChunk, PcmFormat};
use gpui::TestAppContext;
use std::{sync::mpsc, time::Duration};

struct BarrierSink {
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
}
impl AssistantAudioSink for BarrierSink {
    fn write_chunk(&mut self, _: &PcmAudioChunk) -> Result<Duration, String> {
        unreachable!("必须走带实际开播回调的路径")
    }
    fn write_chunk_with_completion(
        &mut self,
        _: &PcmAudioChunk,
        started: async_channel::Sender<()>,
        _: async_channel::Sender<()>,
    ) -> Result<Duration, String> {
        started.try_send(()).unwrap();
        self.entered.send(()).unwrap();
        self.release.recv_timeout(Duration::from_secs(2)).unwrap();
        Ok(Duration::from_millis(20))
    }
    fn stop(&mut self) {}
}

/// 场景：Legacy 报告已实际开播，但调度尚未返回，用户此时插话。
/// 构造：隔离数据库保存报告；20ms × 24kHz × 2B = 960B PCM；sink 在开始信号后阻塞。
/// 执行：真实音频控制器调度，等待开始屏障，前台取消，再允许调度返回并刷新状态。
/// 预期：报告为 Interrupted，不能把尚未消费的硬件开始证据误记为 Skipped。
#[test]
fn legacy_report_cancel_during_scheduling_preserves_hardware_start() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let store = storage::AppStore::initialize(&paths).unwrap();
    let mut test = TestAppContext::single();
    let (app, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    let service = app.update(cx, |app, _| app.shared_context_service().unwrap());
    let runtime = crate::app::shared_tokio_runtime().unwrap();
    let reference = runtime
        .block_on(service.run(None, |access| {
            access.report("legacy-race", "已开播报告", vec![])
        }))
        .unwrap()
        .reference;
    let delivery = service.report_delivery(reference.clone()).unwrap();
    delivery.set(ReportDelivery::Queued);
    let (entered, observed) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    let queue = app.update(cx, |app, cx| {
        let mut announcement = AgentAnnouncement::report("已开播报告".into());
        announcement.delivery = Some(delivery.clone());
        app.start_legacy_announcement_playback(
            app.agent_announcement_playback.generation,
            announcement,
            PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0; 960]).unwrap(),
            move || {
                Box::new(BarrierSink {
                    entered,
                    release: blocked,
                })
            },
            cx,
        );
        Arc::clone(
            &app.agent_announcement_playback
                .control
                .as_ref()
                .unwrap()
                .queue,
        )
    });
    observed.recv_timeout(Duration::from_secs(2)).unwrap();
    app.update(cx, |app, _| app.cancel_agent_announcements());
    release.send(()).unwrap();
    runtime.block_on(queue.wait_stopped());
    runtime.block_on(delivery.flush()).unwrap();
    let status =
        runtime.block_on(service.snapshot()).unwrap().state.reports[&reference.event_id].delivery;
    assert_eq!(
        status,
        ReportDelivery::Interrupted,
        "已开播且插话必须记为中断"
    );
    assert_ne!(
        status,
        ReportDelivery::Skipped,
        "UI 尚未处理 Started 不等于未开播"
    );
}

struct SilentSink {
    finish: bool,
    callbacks: Option<(async_channel::Sender<()>, async_channel::Sender<()>)>,
}
impl AssistantAudioSink for SilentSink {
    fn write_chunk(&mut self, _: &PcmAudioChunk) -> Result<Duration, String> {
        unreachable!("必须走带回调路径")
    }
    fn write_chunk_with_completion(
        &mut self,
        _: &PcmAudioChunk,
        started: async_channel::Sender<()>,
        completed: async_channel::Sender<()>,
    ) -> Result<Duration, String> {
        if self.finish {
            completed.try_send(()).unwrap();
        }
        self.callbacks = Some((started, completed));
        Ok(Duration::from_millis(20))
    }
    fn stop(&mut self) {}
}

/// 场景：Legacy sink 保留 Started sender 却永不发送，完成回调可以到达或同样丢失。
/// 构造：960B = 20ms PCM，worker 截止为 20ms + 2s；两种完成信号情形。
/// 执行：经生产播放入口注入受控 sink，推动前台，等待最多 3s（给线程调度余量）。
/// 预期：两种情形都释放播放；缺失 Started 不得绕过 watchdog，不得伪报 Finished。
#[test]
fn legacy_report_missing_start_does_not_block_completion_watchdog() {
    for finished in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let paths = storage::DatabasePaths::isolated_test(dir.path());
        let store = storage::AppStore::initialize(&paths).unwrap();
        let mut test = TestAppContext::single();
        let (app, cx) =
            test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
        let (observed_events, _wake) = VoiceWindowInsetEventBus::new();
        app.update(cx, |app, cx| {
            app.agent_announcement_playback.connect_playback_event_sink(
                observed_events.sink(TtsPlaybackSource::AgentAnnouncement),
            );
            app.start_legacy_announcement_playback(
                app.agent_announcement_playback.generation,
                AgentAnnouncement::report("回调缺失".into()),
                PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0; 960]).unwrap(),
                move || {
                    Box::new(SilentSink {
                        finish: finished,
                        callbacks: None,
                    })
                },
                cx,
            );
            assert!(
                !app.agent_announcement_playback.is_idle(),
                "实际调度应进入播放阶段"
            );
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            cx.run_until_parked();
            if app.update(cx, |app, _| app.agent_announcement_playback.is_idle()) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "Started 缺失时仍须释放，finished={finished}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        app.update(cx, |app, _| {
            assert!(
                app.agent_announcement_playback.is_idle(),
                "失败后应可继续播报"
            );
        });
        let events = observed_events.drain();
        assert!(
            events
                .iter()
                .any(|(_, e)| matches!(e, TtsPlaybackEvent::PcmQueued(_))),
            "必须确实调度了 PCM，不能让早期拒绝假通过"
        );
        assert!(
            events
                .iter()
                .any(|(_, e)| matches!(e, TtsPlaybackEvent::DiscardQueued)),
            "未开播失败必须清理已排队的能量数据"
        );
        assert!(
            !events
                .iter()
                .any(|(_, e)| matches!(e, TtsPlaybackEvent::Finished)),
            "缺失实际开始不得伪报播完"
        );
        let mut inset = crate::app::voice_window_inset::VoiceWindowInset::default();
        for (source, event) in events {
            inset.apply_playback_event(source, event);
        }
        assert!(
            !inset.is_animation_active(),
            "失败后界面不得因残留 PCM 无限刷新"
        );
    }
}

struct StopBarrierSink {
    scheduled: mpsc::Sender<()>,
    stopping: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
    callbacks: Option<(async_channel::Sender<()>, async_channel::Sender<()>)>,
}
impl AssistantAudioSink for StopBarrierSink {
    fn write_chunk(&mut self, _: &PcmAudioChunk) -> Result<Duration, String> {
        unreachable!("需要硬件回调")
    }
    fn write_chunk_with_completion(
        &mut self,
        _: &PcmAudioChunk,
        started: async_channel::Sender<()>,
        completed: async_channel::Sender<()>,
    ) -> Result<Duration, String> {
        self.callbacks = Some((started, completed));
        self.scheduled.send(()).unwrap();
        Ok(Duration::from_millis(20))
    }
    fn stop(&mut self) {
        if let Some((started, _completed)) = self.callbacks.take() {
            self.stopping.send(()).unwrap();
            self.release.recv_timeout(Duration::from_secs(2)).unwrap();
            let _ = started.try_send(());
        }
    }
}

/// 场景：取消或释放前台播放所有者时，后台监视器仍存活，硬件尚未确认停止。
/// 构造：20ms PCM，sink 在 stop 返回前受屏障控制，并在确认前发送真实开始证据。
/// 执行：走生产播放入口；分别主动取消或释放 owner，检查停止期间不可播下一条，再确认停止。
/// 预期：前台不会阻塞，后台仍会停止；停止前不视作 idle，保存为 Interrupted，迟到 UI 事件不复活播放。
#[test]
fn legacy_report_owner_release_and_cancel_wait_for_hardware_stop() {
    for release_owner in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let paths = storage::DatabasePaths::isolated_test(dir.path());
        let store = storage::AppStore::initialize(&paths).unwrap();
        let mut test = TestAppContext::single();
        let (app, cx) =
            test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
        let service = app.update(cx, |app, _| app.shared_context_service().unwrap());
        let runtime = crate::app::shared_tokio_runtime().unwrap();
        let reference = runtime
            .block_on(service.run(None, |access| access.report("release", "报告", vec![])))
            .unwrap()
            .reference;
        let delivery = service.report_delivery(reference.clone()).unwrap();
        delivery.set(ReportDelivery::Queued);
        let (scheduled, ready) = mpsc::channel();
        let (stopping, stop_started) = mpsc::channel();
        let (release, barrier) = mpsc::channel();
        let queue = app.update(cx, |app, cx| {
            let mut announcement = AgentAnnouncement::report("报告".into());
            announcement.delivery = Some(delivery.clone());
            app.start_legacy_announcement_playback(
                app.agent_announcement_playback.generation,
                announcement,
                PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0; 960]).unwrap(),
                move || {
                    Box::new(StopBarrierSink {
                        scheduled,
                        stopping,
                        release: barrier,
                        callbacks: None,
                    })
                },
                cx,
            );
            Arc::clone(
                &app.agent_announcement_playback
                    .control
                    .as_ref()
                    .unwrap()
                    .queue,
            )
        });
        ready.recv_timeout(Duration::from_secs(2)).unwrap();
        app.update(cx, |app, _| {
            if release_owner {
                drop(app.agent_announcement_playback.control.take());
            } else {
                app.cancel_agent_announcements();
            }
        });
        stop_started.recv_timeout(Duration::from_secs(2)).unwrap();
        app.update(cx, |app, _| {
            assert!(
                !app.agent_announcement_playback.is_idle(),
                "停止确认之前不可开始下一条播报"
            )
        });
        assert!(
            !queue.is_stopped(),
            "后台持有 queue 时，释放前台 owner 仍须启动停止，但不可提前确认"
        );
        release.send(()).unwrap();
        runtime.block_on(queue.wait_stopped());
        runtime.block_on(delivery.flush()).unwrap();
        assert_eq!(
            runtime.block_on(service.snapshot()).unwrap().state.reports[&reference.event_id]
                .delivery,
            ReportDelivery::Interrupted,
            "stop 确认前的开播证据必须保存为中断"
        );
        cx.run_until_parked();
        app.update(cx, |app, _| {
            assert!(
                app.agent_announcement_playback.is_idle(),
                "迟到事件不应复活播报，停止后可继续"
            )
        });
    }
}

struct ReleaseSink {
    ready: mpsc::Sender<std::time::Instant>,
    stopped: Option<mpsc::Sender<()>>,
    callbacks: Option<(async_channel::Sender<()>, async_channel::Sender<()>)>,
}
impl AssistantAudioSink for ReleaseSink {
    fn write_chunk(&mut self, _: &PcmAudioChunk) -> Result<Duration, String> {
        unreachable!("需要真实回调")
    }
    fn write_chunk_with_completion(
        &mut self,
        _: &PcmAudioChunk,
        started: async_channel::Sender<()>,
        completed: async_channel::Sender<()>,
    ) -> Result<Duration, String> {
        self.callbacks = Some((started, completed));
        self.ready.send(std::time::Instant::now()).unwrap();
        Ok(Duration::from_secs(20))
    }
    fn stop(&mut self) {
        self.callbacks.take();
        if let Some(stopped) = self.stopped.take() {
            let _ = stopped.send(());
        }
    }
}

/// 场景：实际 App entity 被释放，但后台 monitor 和测试观察者都持有 queue。
/// 构造：20 秒音频，sink 保留开始/完成 sender；先处理调度事件，排除迟到事件触发清理。
/// 执行：移除测试窗口并释放 App 强引用，不再运行前台 executor，等待硬件停止。
/// 预期：释放后 500ms 内停止，调度至停止总计小于 5 秒（watchdog 为 20 + 2 秒），报告为 Skipped。
/// 此测试验证真实 entity 生命周期，不声称启动过 macOS App。
#[test]
fn legacy_report_releasing_app_stops_audio_without_foreground_callbacks() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let store = storage::AppStore::initialize(&paths).unwrap();
    let mut test = TestAppContext::single();
    let (app, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    let service = app.update(cx, |app, _| app.shared_context_service().unwrap());
    let runtime = crate::app::shared_tokio_runtime().unwrap();
    let reference = runtime
        .block_on(service.run(None, |access| {
            access.report("app-release", "待播放报告", vec![])
        }))
        .unwrap()
        .reference;
    let delivery = service.report_delivery(reference.clone()).unwrap();
    delivery.set(ReportDelivery::Queued);
    let (ready, scheduled) = mpsc::channel();
    let (stopped, stop) = mpsc::channel();
    let queue = app.update(cx, |app, cx| {
        let mut announcement = AgentAnnouncement::report("待播放报告".into());
        announcement.delivery = Some(delivery.clone());
        app.start_legacy_announcement_playback(
            app.agent_announcement_playback.generation,
            announcement,
            // 24,000 samples/s × 2 bytes/sample × 20s = 960,000 bytes.
            PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0; 960_000]).unwrap(),
            move || {
                Box::new(ReleaseSink {
                    ready,
                    stopped: Some(stopped),
                    callbacks: None,
                })
            },
            cx,
        );
        Arc::clone(
            &app.agent_announcement_playback
                .control
                .as_ref()
                .unwrap()
                .queue,
        )
    });
    let scheduled_at = scheduled.recv_timeout(Duration::from_secs(2)).unwrap();
    cx.run_until_parked();
    assert!(
        matches!(stop.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "释放 App 前音频必须仍未停止，排除 watchdog 已触发造成假阳性"
    );
    let weak = app.downgrade();
    cx.update(|window, _| window.remove_window());
    drop(app);
    test.update(|_| {});
    assert!(
        weak.upgrade().is_none(),
        "测试必须真正释放 App entity，不能只取走 control"
    );
    stop.recv_timeout(Duration::from_millis(500))
        .expect("App drop 必须立即停止，不能等 watchdog 或前台回调");
    assert!(
        scheduled_at.elapsed() < Duration::from_secs(5),
        "调度、前台清空和释放的总耗时必须远小于 22 秒 watchdog"
    );
    runtime.block_on(queue.wait_stopped());
    runtime.block_on(delivery.flush()).unwrap();
    assert_eq!(
        runtime.block_on(service.snapshot()).unwrap().state.reports[&reference.event_id].delivery,
        ReportDelivery::Skipped,
        "未开播报告在 App 释放后不得继续播放或假报完成"
    );
}
