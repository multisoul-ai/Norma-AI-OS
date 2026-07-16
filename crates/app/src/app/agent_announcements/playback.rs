use super::AgentAnnouncement;
use crate::app::dialogue::{create_assistant_audio_sink, TtsClientBox};
use crate::app::voice::playback::AssistantAudioSink;
use crate::app::voice_window_inset::TtsPlaybackEvent;
use canvas::NodeId;
use gpui::BackgroundExecutor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const COMPLETION_CALLBACK_GRACE: Duration = Duration::from_secs(2);

pub(super) fn completion_timeout_for_chunk(chunk: &audio::PcmAudioChunk) -> Duration {
    crate::app::voice::playback::pcm_chunk_duration(chunk).saturating_add(COMPLETION_CALLBACK_GRACE)
}

pub(super) async fn await_playback_completion_or_timeout(
    executor: BackgroundExecutor,
    finished_rx: async_channel::Receiver<()>,
    timeout: Duration,
) -> bool {
    let (outcome_tx, outcome_rx) = async_channel::bounded(1);
    let completion_outcome_tx = outcome_tx.clone();
    executor
        .spawn(async move {
            let completed = finished_rx.recv().await.is_ok();
            let _ = completion_outcome_tx.try_send(completed);
        })
        .detach();
    let timeout_task = executor.timer(timeout);
    executor
        .spawn(async move {
            timeout_task.await;
            let _ = outcome_tx.try_send(false);
        })
        .detach();
    outcome_rx.recv().await.unwrap_or(false)
}

pub(super) struct AgentAnnouncementAudioControl {
    cancelled: AtomicBool,
    sink: Mutex<Option<Box<dyn AssistantAudioSink>>>,
}

impl AgentAnnouncementAudioControl {
    pub(super) fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            sink: Mutex::new(None),
        }
    }

    pub(super) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        if let Ok(mut current) = self.sink.try_lock() {
            if let Some(mut sink) = current.take() {
                sink.stop();
                let _ = std::thread::Builder::new()
                    .name("agent_announcement_audio_release".to_string())
                    .spawn(move || drop(sink));
            }
        }
    }

    pub(super) fn play_with_completion(
        &self,
        chunk: &audio::PcmAudioChunk,
    ) -> Result<(async_channel::Receiver<()>, async_channel::Receiver<()>), String> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err("Agent announcement was cancelled".to_string());
        }
        let mut sink = create_assistant_audio_sink();
        let (started_tx, started_rx) = async_channel::bounded(1);
        let (finished_tx, finished_rx) = async_channel::bounded(1);
        sink.write_chunk_with_completion(chunk, started_tx, finished_tx)?;
        if self.cancelled.load(Ordering::Acquire) {
            sink.stop();
            return Err("Agent announcement was cancelled".to_string());
        }
        let mut current = self
            .sink
            .lock()
            .map_err(|_| "Agent announcement sink lock was poisoned".to_string())?;
        if self.cancelled.load(Ordering::Acquire) {
            sink.stop();
            return Err("Agent announcement was cancelled".to_string());
        }
        *current = Some(sink);
        Ok((started_rx, finished_rx))
    }
}

pub(super) enum AgentAnnouncementPlaybackPhase {
    Idle,
    Synthesizing(AgentAnnouncement),
    Playing {
        announcement: AgentAnnouncement,
        started: bool,
    },
}

pub(crate) struct AgentAnnouncementPlayback {
    pub(crate) generation: u64,
    pub(super) phase: AgentAnnouncementPlaybackPhase,
    pub(super) control: Option<Arc<AgentAnnouncementAudioControl>>,
    pub(super) tts_client: Option<TtsClientBox>,
    pub(super) playback_events: Vec<TtsPlaybackEvent>,
    #[cfg(feature = "e2e-test-support")]
    pub(crate) e2e_enabled: bool,
    #[cfg(feature = "e2e-test-support")]
    pub(crate) e2e_spoken: Vec<String>,
}

impl Default for AgentAnnouncementPlayback {
    fn default() -> Self {
        Self {
            generation: 0,
            phase: AgentAnnouncementPlaybackPhase::Idle,
            control: None,
            tts_client: None,
            playback_events: Vec::new(),
            #[cfg(feature = "e2e-test-support")]
            e2e_enabled: false,
            #[cfg(feature = "e2e-test-support")]
            e2e_spoken: Vec::new(),
        }
    }
}

impl AgentAnnouncementPlayback {
    pub(super) fn is_idle(&self) -> bool {
        matches!(self.phase, AgentAnnouncementPlaybackPhase::Idle)
    }

    pub(super) fn active_node_id(&self) -> Option<NodeId> {
        match &self.phase {
            AgentAnnouncementPlaybackPhase::Idle => None,
            AgentAnnouncementPlaybackPhase::Synthesizing(announcement)
            | AgentAnnouncementPlaybackPhase::Playing { announcement, .. } => {
                Some(announcement.node_id)
            }
        }
    }

    pub(super) fn cancel_active(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        let had_active = !matches!(self.phase, AgentAnnouncementPlaybackPhase::Idle);
        let started = matches!(
            self.phase,
            AgentAnnouncementPlaybackPhase::Playing { started: true, .. }
        );
        if let Some(control) = self.control.take() {
            control.cancel();
        }
        self.playback_events.retain(|event| {
            !matches!(
                event,
                TtsPlaybackEvent::PcmQueued(_) | TtsPlaybackEvent::Started
            )
        });
        if started {
            self.playback_events.push(TtsPlaybackEvent::Interrupted);
        } else if had_active {
            self.playback_events.push(TtsPlaybackEvent::DiscardQueued);
        }
        self.phase = AgentAnnouncementPlaybackPhase::Idle;
    }

    pub(crate) fn take_playback_events(&mut self) -> Vec<TtsPlaybackEvent> {
        std::mem::take(&mut self.playback_events)
    }

    pub(super) fn take_tts_client_for_configuration_change(&mut self) -> Option<TtsClientBox> {
        self.tts_client.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// Agent 播报完成回调丢失：等待器必须在截止时间释放，不能永久阻塞后续播报。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   completion channel capacity = 1（保留 sender，但永不发送，模拟回调丢失）
    ///   timeout                     = 2_100ms（测试用明确截止时间）
    ///   before deadline             = 2_099ms = 2_100ms - 1ms
    ///   deadline step               = 1ms，累计恰好 2_100ms
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 启动 completion-or-timeout 等待器，保持 completion sender 存活且不发送。
    ///   2. 推进 2_099ms → 等待器仍应 pending，不能提前判定失败。
    ///   3. 再推进 1ms → timeout 胜出并返回 completed=false。
    ///
    /// 预期结果：
    ///   - 负断言：截止前 outcome 仍为 None，不能提前切断正常音频。
    ///   - 正断言：截止时 outcome 为 Some(false)，后续播报可继续调度。
    ///   - 负断言：截止时不能返回 Some(true)，因为真实完成回调从未到达。
    #[test]
    fn test_missing_agent_playback_completion_times_out_instead_of_blocking_forever() {
        let mut cx = TestAppContext::single();
        let (_completion_tx, completion_rx) = async_channel::bounded(1);
        let outcome = Arc::new(Mutex::new(None));
        let task_outcome = Arc::clone(&outcome);
        let executor = cx.background_executor.clone();
        let timeout = Duration::from_millis(2_100);

        cx.spawn(move |_cx| async move {
            let completed =
                await_playback_completion_or_timeout(executor, completion_rx, timeout).await;
            *task_outcome
                .lock()
                .expect("Agent 播报 timeout 测试 outcome lock 不应 poisoned") = Some(completed);
        })
        .detach();
        cx.run_until_parked();

        cx.background_executor
            .advance_clock(Duration::from_millis(2_099));
        cx.run_until_parked();
        assert_eq!(
            *outcome
                .lock()
                .expect("截止前读取 outcome lock 不应 poisoned"),
            None,
            "2_099ms 尚未达到 2_100ms 截止时间，等待器不应提前返回"
        );

        cx.background_executor
            .advance_clock(Duration::from_millis(1));
        cx.run_until_parked();
        let completed = *outcome
            .lock()
            .expect("截止时读取 outcome lock 不应 poisoned");
        assert_eq!(
            completed,
            Some(false),
            "completion 回调缺失时必须在 2_100ms 截止点返回未完成"
        );
        assert_ne!(
            completed,
            Some(true),
            "没有 completion 信号时不得伪报真实播放完成"
        );
    }

    /// Agent 播报正常完成：真实回调必须优先于看门狗并立即报告成功。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   completion channel capacity = 1（测试启动前写入 1 个完成信号）
    ///   queued signals               = 1 = channel capacity
    ///   timeout                      = 2_100ms（晚于已排队的完成信号）
    ///   advanced clock               = 0ms（完成路径不应依赖推进时钟）
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 先向 completion channel 写入真实完成信号。
    ///   2. 启动 completion-or-timeout 等待器并运行到 parked。
    ///   3. completion 分支先返回 completed=true，2_100ms 看门狗不参与结果。
    ///
    /// 预期结果：
    ///   - 正断言：outcome 为 Some(true)，真实回调被识别为正常完成。
    ///   - 负断言：outcome 不得为 Some(false)，不能把正常播完误判为中断。
    ///   - 负断言：outcome 不得为 None，不能无故等待到超时。
    #[test]
    fn test_agent_playback_completion_callback_wins_before_timeout() {
        let mut cx = TestAppContext::single();
        let (completion_tx, completion_rx) = async_channel::bounded(1);
        completion_tx
            .try_send(())
            .expect("capacity=1 的空 channel 应接受唯一的完成信号");
        let outcome = Arc::new(Mutex::new(None));
        let task_outcome = Arc::clone(&outcome);
        let executor = cx.background_executor.clone();
        let timeout = Duration::from_millis(2_100);

        cx.spawn(move |_cx| async move {
            let completed =
                await_playback_completion_or_timeout(executor, completion_rx, timeout).await;
            *task_outcome
                .lock()
                .expect("Agent 播报 completion 测试 outcome lock 不应 poisoned") = Some(completed);
        })
        .detach();
        cx.run_until_parked();

        let completed = *outcome
            .lock()
            .expect("读取 completion 测试 outcome lock 不应 poisoned");
        assert_eq!(
            completed,
            Some(true),
            "真实 completion 信号已排队时应立即报告播放完成"
        );
        assert_ne!(
            completed,
            Some(false),
            "真实 completion 信号不得被看门狗误判为中断"
        );
        assert_ne!(
            completed, None,
            "真实 completion 信号已就绪时等待器不应继续 pending"
        );
    }
}
