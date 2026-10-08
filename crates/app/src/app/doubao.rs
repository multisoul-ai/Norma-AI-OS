use crate::app::MultisoulApp;
use gpui::{Context, Window};
use tokio_util::sync::CancellationToken;
use ui::i18n::LocalizedText;

mod input;
mod plan;
mod playback;
mod tools;

use plan::run_session;

pub(crate) struct DoubaoReport {
    pub(crate) text: String,
    pub(crate) cancellation: CancellationToken,
    pub(crate) completed: tokio::sync::oneshot::Sender<()>,
}

impl DoubaoReport {
    pub(crate) fn new(text: String) -> (Self, tokio::sync::oneshot::Receiver<()>) {
        let (completed, completion) = tokio::sync::oneshot::channel();
        (
            Self {
                text,
                cancellation: CancellationToken::new(),
                completed,
            },
            completion,
        )
    }
}

pub(super) type ContextRequests = async_channel::Sender<
    tokio::sync::oneshot::Sender<(agent::CurrentCanvasContext, Vec<agent::AgentRosterEntry>)>,
>;

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
    recording_id: u64,
    awaiting_transcript: bool,
    recordings: Option<async_channel::Sender<input::RecordingInput>>,
    speech: Option<async_channel::Sender<plan::asr::SpeechEvent>>,
    activation: Option<gpui::Subscription>,
    session_task: Option<tokio::task::JoinHandle<()>>,
    pub(crate) reports: Option<async_channel::Sender<DoubaoReport>>,
}

impl Default for DoubaoVoiceState {
    fn default() -> Self {
        Self {
            status: DoubaoStatus::Idle,
            generation: 0,
            cancellation: None,
            recording: None,
            recording_from_shortcut: false,
            recording_id: 0,
            awaiting_transcript: false,
            recordings: None,
            speech: None,
            activation: None,
            session_task: None,
            reports: None,
        }
    }
}

impl DoubaoVoiceState {
    pub(crate) fn apply_status(&mut self, status: DoubaoStatus) {
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
        self.reports = None;
        if let Some(recording) = self.recording.take() {
            recording.cancel();
        }
        self.recording_from_shortcut = false;
        self.awaiting_transcript = false;
        self.recordings = None;
        self.speech = None;
        self.activation = None;
        self.status = DoubaoStatus::Idle;
    }

    pub(crate) fn is_active(&self) -> bool {
        self.cancellation.is_some()
    }

    pub(crate) fn waiting_for_input(&self) -> bool {
        self.status == DoubaoStatus::Connected
            && self.recordings.is_some()
            && self.recording.is_none()
    }

    pub(crate) fn label(&self) -> &'static str {
        if self.waiting_for_input() {
            ui::i18n::text("voice.push_to_talk")
        } else {
            self.status.label()
        }
    }
}

impl MultisoulApp {
    fn start_doubao_context_requests(
        &self,
        generation: u64,
        project_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ContextRequests {
        let (requests, incoming): (ContextRequests, _) = async_channel::bounded(1);
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(response) = incoming.recv().await {
                let context = this
                    .update_in(cx, |app, _, _| {
                        let current_project = app
                            .runtime
                            .workspace_root()
                            .map(|root| root.display().to_string());
                        if app.doubao.generation != generation || current_project != project_id {
                            return None;
                        }
                        let canvas =
                            app.current_canvas_context_for_provider()
                                .unwrap_or_else(|| agent::CurrentCanvasContext {
                                    project_name: "未打开项目".into(),
                                    workspace_root: "未打开项目；项目操作不可用".into(),
                                    total_cards: 0,
                                    cards: Vec::new(),
                                    conversation_tails: Vec::new(),
                                });
                        Some((canvas, app.agent_roster_for_provider()))
                    })
                    .ok()
                    .flatten();
                let Some(context) = context else {
                    break;
                };
                let _ = response.send(context);
            }
        })
        .detach();
        requests
    }

    pub(crate) fn stop_doubao_voice(&mut self, cx: &mut Context<Self>) {
        self.doubao.stop();
        cx.notify();
    }

    pub(crate) fn toggle_doubao_voice(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.doubao.is_active() {
            self.stop_doubao_voice(cx);
            return;
        }
        let Some(account) = self
            .voice_settings
            .values()
            .doubao_credential_ref()
            .map(str::to_owned)
        else {
            self.doubao.status =
                DoubaoStatus::Disconnected(LocalizedText::new("doubao.key_required"));
            cx.notify();
            return;
        };
        let project_id = self
            .runtime
            .workspace_root()
            .map(|root| root.display().to_string());
        let bound_project = project_id.clone();
        self.doubao.generation = self.doubao.generation.wrapping_add(1);
        let generation = self.doubao.generation;
        let tool_adapter = super::agent_runtime::start_doubao_adapter(
            project_id.clone().unwrap_or_default(),
            generation,
            self.database_paths.clone(),
            self.workspace_state.current_root.clone(),
            window,
            cx,
        );
        let tools = Some(agent::NormaToolRuntime::new(tool_adapter));
        let database = self.database_paths.app_db_path().to_path_buf();
        let selected_voice = self.voice_settings.values().doubao_voice();
        let cancellation = CancellationToken::new();
        self.doubao.cancellation = Some(cancellation.clone());
        self.doubao.status = DoubaoStatus::Connecting;
        tracing::info!(target: "norma_voice", generation,
            "Doubao session start requested");
        let (events_tx, events_rx) = async_channel::bounded(32);
        let (speech_tx, speech_rx) = async_channel::unbounded();
        self.doubao.speech = Some(speech_tx.clone());
        let (reports_tx, reports_rx) = async_channel::unbounded();
        let recordings = if self.voice_settings.values().doubao_interaction_mode()
            == crate::settings::voice::VoiceInteractionMode::SingleVoiceCommand
        {
            let (sender, receiver) = async_channel::unbounded();
            self.doubao.recordings = Some(sender);
            Some(receiver)
        } else {
            None
        };
        let input = input::SessionInput {
            events: speech_tx,
            speech: speech_rx,
            recordings,
        };
        self.doubao.activation = Some(cx.observe_window_activation(window, |app, window, cx| {
            if !window.is_window_active() && app.doubao.recording.is_some() {
                // 失活时可能收不到松键事件；结束采集后仍须完成当前转写和回复。
                app.end_doubao_recording(cx);
            }
        }));
        self.doubao.reports = Some(reports_tx);
        let playback_events = self
            .voice_window_inset_events
            .sink(crate::app::voice_window_inset::TtsPlaybackSource::Doubao);
        if let Ok(runtime) = super::shared_tokio_runtime() {
            let context =
                self.start_doubao_context_requests(generation, project_id.clone(), window, cx);
            let previous_task = self.doubao.session_task.take();
            self.doubao.session_task = Some(runtime.spawn(async move {
                if let Some(previous_task) = previous_task {
                    tracing::info!(target: "norma_voice", generation,
                        "waiting for previous Doubao session to finish");
                    let _ = previous_task.await;
                    tracing::info!(target: "norma_voice", generation,
                        "previous Doubao session finished");
                }
                if cancellation.is_cancelled() {
                    return;
                }
                let result = run_session(
                    (account, selected_voice),
                    context,
                    project_id,
                    database,
                    tools,
                    cancellation,
                    (events_tx.clone(), reports_rx, playback_events, input),
                )
                .await;
                tracing::info!(target: "norma_voice", generation,
                    reason = result.as_ref().err().map(LocalizedText::as_str).unwrap_or("normal"),
                    "Doubao session task finished");
                let _ = events_tx
                    .send(DoubaoStatus::Disconnected(
                        result
                            .err()
                            .unwrap_or_else(|| LocalizedText::new("doubao.call_ended")),
                    ))
                    .await;
            }));
        } else {
            self.doubao.stop();
            self.doubao.status =
                DoubaoStatus::Disconnected(LocalizedText::new("doubao.runtime_unavailable"));
            cx.notify();
            return;
        }
        cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(100))
                .await;
            let still_active = this
                .update_in(cx, |app, _, cx| {
                    if app.doubao.generation != generation {
                        return false;
                    }
                    let now = app
                        .runtime
                        .workspace_root()
                        .map(|root| root.display().to_string());
                    if now != bound_project {
                        app.stop_doubao_voice(cx);
                        return false;
                    }
                    while let Ok(status) = events_rx.try_recv() {
                        let ended = matches!(status, DoubaoStatus::Disconnected(_));
                        if let DoubaoStatus::Disconnected(message) = &status {
                            tracing::info!(target: "norma_voice", reason = %message, "voice conversation ended");
                        }
                        app.doubao.apply_status(status);
                        if ended {
                            if let Some(recording) = app.doubao.recording.take() { recording.cancel(); }
                            app.doubao.recording_from_shortcut = false;
                            app.doubao.awaiting_transcript = false;
                            app.doubao.recordings = None;
                            app.doubao.speech = None;
                            app.doubao.activation = None;
                            app.doubao.cancellation = None;
                            app.doubao.reports = None;
                        }
                        cx.notify();
                    }
                    app.doubao.is_active()
                })
                .unwrap_or(false);
            if !still_active {
                break;
            }
        })
        .detach();
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
