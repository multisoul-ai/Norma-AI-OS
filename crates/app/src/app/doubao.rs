use crate::app::MultisoulApp;
use gpui::Context;
use std::time::Instant;
use tokio_util::sync::CancellationToken;
use ui::i18n::LocalizedText;

mod channel;
mod input;
mod plan;
pub(super) mod playback;
mod session;
mod tools;

pub(crate) struct DoubaoReport {
    pub(crate) text: String,
    pub(crate) cancellation: CancellationToken,
    pub(crate) completed: tokio::sync::oneshot::Sender<()>,
    /// 超过该时刻仍未开播即丢弃（通知有效期）。
    pub(crate) expires_at: Option<Instant>,
}

impl DoubaoReport {
    pub(crate) fn new(text: String) -> (Self, tokio::sync::oneshot::Receiver<()>) {
        let (completed, completion) = tokio::sync::oneshot::channel();
        (
            Self {
                text,
                cancellation: CancellationToken::new(),
                completed,
                expires_at: None,
            },
            completion,
        )
    }
}

pub(super) enum ContextRequest {
    Model(
        tokio::sync::oneshot::Sender<(
            agent::CurrentCanvasContext,
            Vec<agent::AgentRosterEntry>,
            Option<agent::DesktopContext>,
        )>,
    ),
}

pub(super) type ContextRequests = async_channel::Sender<ContextRequest>;

fn connect_failure_message(error: &tokio_tungstenite::tungstenite::Error) -> LocalizedText {
    match error {
        tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == 401 => {
            LocalizedText::new("doubao.auth_failed")
        }
        tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == 403 => {
            LocalizedText::new("doubao.resource_denied")
        }
        _ => LocalizedText::new("doubao.connection_failed"),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DoubaoStatus {
    Idle,
    Connecting,
    Connected,
    Thinking,
    Speaking,
    RecordingResolved(u64),
    UserTranscript(String),
    AssistantReply(String),
    /// 一次收音结束（事件，不作为持久状态）；`Some` 为失败原因。
    ListenEnded(u64, Option<LocalizedText>),
    /// 收音失败，通道仍在；下次 Cmd+E 清除。
    ListenFailed(LocalizedText),
    Disconnected(LocalizedText),
}

impl DoubaoStatus {
    pub(crate) fn label(&self) -> &'static str {
        ui::i18n::text(match self {
            Self::Idle => "doubao.voice",
            Self::Connecting => "voice.connecting",
            Self::Connected => "chrome.listening",
            Self::Thinking => "doubao.thinking",
            Self::Speaking => "doubao.replying",
            Self::RecordingResolved(_) => "voice.push_to_talk",
            Self::UserTranscript(_) | Self::AssistantReply(_) | Self::ListenEnded(..) => {
                "chrome.listening"
            }
            Self::ListenFailed(_) => "doubao.listen_failed",
            Self::Disconnected(_) => "doubao.disconnected",
        })
    }
}

pub(crate) struct DoubaoVoiceState {
    pub(crate) status: DoubaoStatus,
    pub(crate) generation: u64,
    pub(crate) cancellation: Option<CancellationToken>,
    pub(crate) recording: Option<CancellationToken>,
    pub(crate) recording_from_shortcut: bool,
    /// 持续聆听：本次 Cmd+E 收音的令牌。
    pub(crate) listening: Option<CancellationToken>,
    recording_id: u64,
    awaiting_transcript: bool,
    recordings: Option<async_channel::Sender<input::RecordingInput>>,
    listens: Option<async_channel::Sender<input::ListenRequest>>,
    binding: Option<channel::ChannelBinding>,
    start_pending: bool,
    retry_attempt: u32,
    retry_at: Option<Instant>,
    /// 本次通道首次就绪的时刻；用来判断断线前是否稳定连接过。
    connected_at: Option<Instant>,
    blocked: bool,
    speech: Option<async_channel::Sender<plan::asr::SpeechEvent>>,
    activation: Option<gpui::Subscription>,
    session_task: Option<tokio::task::JoinHandle<()>>,
    pub(crate) reports: Option<async_channel::Sender<DoubaoReport>>,
    pub(crate) last_transcript: Option<String>,
    pub(crate) last_reply: Option<String>,
}

impl Default for DoubaoVoiceState {
    fn default() -> Self {
        Self {
            status: DoubaoStatus::Idle,
            generation: 0,
            cancellation: None,
            recording: None,
            recording_from_shortcut: false,
            listening: None,
            recording_id: 0,
            awaiting_transcript: false,
            recordings: None,
            listens: None,
            binding: None,
            start_pending: false,
            retry_attempt: 0,
            retry_at: None,
            connected_at: None,
            blocked: false,
            speech: None,
            activation: None,
            session_task: None,
            reports: None,
            last_transcript: None,
            last_reply: None,
        }
    }
}

impl DoubaoVoiceState {
    pub(crate) fn apply_status(&mut self, status: DoubaoStatus) {
        if let DoubaoStatus::UserTranscript(text) = status {
            self.last_transcript = Some(text);
            self.last_reply = None;
            return;
        }
        if let DoubaoStatus::AssistantReply(text) = status {
            self.last_reply = Some(text);
            self.retry_attempt = 0;
            return;
        }
        if let DoubaoStatus::ListenEnded(id, error) = status {
            self.end_listen(id, error);
            return;
        }
        if status == DoubaoStatus::Connected && self.connected_at.is_none() {
            self.connected_at = Some(Instant::now());
        }
        if let DoubaoStatus::RecordingResolved(id) = status {
            if id == self.recording_id {
                self.awaiting_transcript = false;
            }
            return;
        }
        self.status =
            if self.recording.is_some() && !matches!(status, DoubaoStatus::Disconnected(_)) {
                DoubaoStatus::Connected
            } else if self.awaiting_transcript && !matches!(status, DoubaoStatus::Disconnected(_)) {
                DoubaoStatus::Thinking
            } else {
                status
            };
    }

    pub(crate) fn stop(&mut self) {
        if self.is_active() {
            tracing::info!(target: "norma_voice", generation = self.generation,
                "Doubao session cancellation requested");
        }
        self.generation = self.generation.wrapping_add(1);
        if let Some(cancellation) = self.cancellation.take() {
            cancellation.cancel();
        }
        self.end_channel();
        self.start_pending = false;
        self.retry_attempt = 0;
        self.retry_at = None;
        self.connected_at = None;
        self.blocked = false;
        self.status = DoubaoStatus::Idle;
    }

    pub(crate) fn is_active(&self) -> bool {
        self.cancellation.is_some()
    }

    /// 通道已就绪但没有收音：待命。
    pub(crate) fn waiting_for_input(&self) -> bool {
        self.status == DoubaoStatus::Connected && !self.is_listening()
    }

    pub(crate) fn label(&self) -> &'static str {
        if self.waiting_for_input() {
            ui::i18n::text(if self.recordings.is_some() {
                "voice.push_to_talk"
            } else {
                "doubao.voice"
            })
        } else {
            self.status.label()
        }
    }
}

impl MultisoulApp {
    pub(crate) fn stop_doubao_voice(&mut self, cx: &mut Context<Self>) {
        self.doubao.stop();
        cx.notify();
    }
}

#[cfg(test)]
#[path = "../../tests/unit/doubao_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/unit/doubao_visual_tests.rs"]
mod visual_tests;

#[cfg(test)]
#[path = "../../tests/unit/doubao_memory_tests.rs"]
mod memory_tests;

#[cfg(test)]
#[path = "../../tests/unit/doubao_language_tests.rs"]
mod language_tests;

#[cfg(test)]
#[path = "../../tests/unit/doubao_context_tests.rs"]
mod context_tests;

#[cfg(test)]
#[path = "../../tests/unit/doubao_budget_tests.rs"]
mod budget_tests;
