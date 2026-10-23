//! 豆包常驻播报通道：项目打开即启动并后台保持，Cmd+E 只开关收音。
use super::{DoubaoStatus, DoubaoVoiceState};
use crate::app::MultisoulApp;
use gpui::Context;
use std::time::{Duration, Instant};
use ui::i18n::LocalizedText;

#[cfg(test)]
#[path = "../../../tests/unit/doubao_channel_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/doubao_channel_app_tests.rs"]
mod app_tests;

const MAX_RETRY_DELAY_SECS: u64 = 60;

/// 鉴权与凭据问题重连也不会成功；等用户修改设置或手动按 Cmd+E。
const PERMANENT_FAILURES: [&str; 7] = [
    "doubao.auth_failed",
    "doubao.resource_denied",
    "doubao.key_required",
    "doubao.credential_missing",
    "doubao.credential_unavailable",
    "doubao.chat_denied",
    "doubao.tts_denied",
];

/// 通道启动时所绑定的项目与窗口；任一变化都需要重建通道。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ChannelBinding {
    pub(crate) project: Option<String>,
    pub(crate) window: Option<u64>,
}

#[derive(Clone, Debug)]
pub(super) struct ChannelInputs<'a> {
    pub(super) doubao_mode: bool,
    pub(super) credential: bool,
    pub(super) project: Option<&'a str>,
    pub(super) window: Option<u64>,
    pub(super) now: Instant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ChannelAction {
    Keep,
    Start,
    Restart,
    Stop,
}

pub(super) fn channel_action(
    state: &DoubaoVoiceState,
    inputs: &ChannelInputs<'_>,
) -> ChannelAction {
    let wanted = inputs.doubao_mode && inputs.credential && inputs.project.is_some();
    if state.is_active() {
        if !wanted {
            return ChannelAction::Stop;
        }
        let Some(binding) = &state.binding else {
            return ChannelAction::Keep;
        };
        let rebind = if binding.project.as_deref() != inputs.project {
            true
        } else {
            inputs.window.is_some() && binding.window != inputs.window
        };
        return match (rebind, inputs.window.is_some()) {
            (false, _) => ChannelAction::Keep,
            (true, true) => ChannelAction::Restart,
            (true, false) => ChannelAction::Stop,
        };
    }
    let waiting = state.start_pending
        || state.blocked
        || state.retry_at.is_some_and(|retry_at| inputs.now < retry_at);
    if wanted && inputs.window.is_some() && !waiting {
        ChannelAction::Start
    } else {
        ChannelAction::Keep
    }
}

/// 1、2、4…秒翻倍，封顶 60 秒。
pub(super) fn retry_delay(attempt: u32) -> Duration {
    let seconds = 1_u64
        .checked_shl(attempt)
        .unwrap_or(u64::MAX)
        .min(MAX_RETRY_DELAY_SECS);
    Duration::from_secs(seconds)
}

pub(super) fn is_permanent_failure(reason: &LocalizedText) -> bool {
    PERMANENT_FAILURES
        .iter()
        .any(|key| *reason == LocalizedText::new(key))
}

impl DoubaoVoiceState {
    pub(crate) fn is_listening(&self) -> bool {
        self.recording.is_some() || self.listening.is_some()
    }

    pub(crate) fn is_blocked(&self) -> bool {
        self.blocked
    }

    #[cfg(test)]
    pub(crate) fn block_for_test(&mut self) {
        self.note_channel_ended(&LocalizedText::new("doubao.auth_failed"), Instant::now());
    }

    /// 通道断开：释放收音和通道句柄，但保留退避状态（`stop()` 会清空它）。
    pub(super) fn end_channel(&mut self) {
        if let Some(recording) = self.recording.take() {
            recording.cancel();
        }
        if let Some(listening) = self.listening.take() {
            listening.cancel();
        }
        self.recording_from_shortcut = false;
        self.awaiting_transcript = false;
        self.recordings = None;
        self.listens = None;
        self.speech = None;
        self.activation = None;
        self.cancellation = None;
        self.reports = None;
        self.binding = None;
    }

    pub(super) fn note_channel_ended(&mut self, reason: &LocalizedText, now: Instant) {
        if is_permanent_failure(reason) {
            self.blocked = true;
            self.retry_at = None;
            return;
        }
        self.retry_at = Some(now + retry_delay(self.retry_attempt));
        self.retry_attempt = self.retry_attempt.saturating_add(1);
    }

    pub(super) fn end_listen(&mut self, id: u64, error: Option<LocalizedText>) {
        if id != self.recording_id {
            return;
        }
        if let Some(listening) = self.listening.take() {
            listening.cancel();
        }
        if let Some(recording) = self.recording.take() {
            recording.cancel();
        }
        self.recording_from_shortcut = false;
        self.awaiting_transcript = false;
        if let Some(error) = error {
            self.status = DoubaoStatus::ListenFailed(error);
        }
    }
}

impl MultisoulApp {
    /// 每个运行时帧调用：只做字段比较，需要启动时经项目窗口句柄异步拿到 `Window`。
    pub(crate) fn reconcile_doubao_channel(&mut self, cx: &mut Context<Self>) {
        let values = self.voice_settings.values();
        let project = self
            .runtime
            .workspace_root()
            .map(|root| root.display().to_string());
        let inputs = ChannelInputs {
            doubao_mode: values.desktop_mode() == crate::settings::voice::DesktopVoiceMode::Doubao,
            credential: values.doubao_credential_ref().is_some(),
            project: project.as_deref(),
            window: self
                .project_window
                .map(|window| window.window_id().as_u64()),
            now: Instant::now(),
        };
        match channel_action(&self.doubao, &inputs) {
            ChannelAction::Keep => {}
            ChannelAction::Stop => {
                tracing::info!(target: "norma_voice", "Doubao report channel no longer wanted");
                self.stop_doubao_voice(cx);
            }
            ChannelAction::Restart => {
                tracing::info!(target: "norma_voice", "Doubao report channel rebinding");
                self.stop_doubao_voice(cx);
                self.schedule_doubao_channel_start(cx);
            }
            ChannelAction::Start => self.schedule_doubao_channel_start(cx),
        }
    }

    fn schedule_doubao_channel_start(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.project_window else {
            return;
        };
        self.doubao.start_pending = true;
        cx.spawn(async move |this, cx| {
            let started = handle.update(cx, |_, window, cx| {
                this.update(cx, |app, cx| {
                    app.doubao.start_pending = false;
                    if !app.doubao.is_active() {
                        app.start_doubao_channel(window, cx);
                    }
                })
            });
            if !matches!(started, Ok(Ok(()))) {
                let _ = this.update(cx, |app, _| app.doubao.start_pending = false);
            }
        })
        .detach();
    }

    /// 只停止收音，播报通道继续常驻（停止按钮、项目转入后台）。
    pub(crate) fn stop_doubao_listening(&mut self, cx: &mut Context<Self>) {
        if let Some(listening) = self.doubao.listening.take() {
            listening.cancel();
        }
        if self.doubao.recording.is_some() {
            self.end_doubao_recording(cx);
        }
        if matches!(self.doubao.status, DoubaoStatus::ListenFailed(_)) {
            self.doubao.status = DoubaoStatus::Connected;
        }
        cx.notify();
    }
}
