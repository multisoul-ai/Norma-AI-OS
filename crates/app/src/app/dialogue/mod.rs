//! App-layer shell state for Always Listening local voice dialogue.

mod chat;
mod reply;
mod shell;

use crate::app::voice::control::VoiceVisual;
use crate::settings::voice::VoiceSettingsValues;
use dialogue::{apply_asr_final, handle_barge_in_speech_start, DialoguePhase, DialogueSession};
use gpui::{Context, Task};
use reply::{
    apply_stub_reply_synthesis, create_assistant_audio_sink, create_tts_client,
    finish_stub_reply_playback, StubReplyPlayback, TtsClientBox,
};
use std::fmt;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(test)]
use std::sync::Arc;
use std::sync::Mutex;
use voice_sherpa::{ListeningPipelineEvent, PcmStreamSession, SherpaProvider, SpeechError};

type MicPermissionCheck = fn() -> Result<(), SpeechError>;

static MIC_PERMISSION_CHECK: Mutex<Option<MicPermissionCheck>> = Mutex::new(None);

pub(crate) fn default_mic_permission_check() -> Result<(), SpeechError> {
    voice_sherpa::request_microphone_permission()
}

pub(crate) fn check_mic_permission() -> Result<(), SpeechError> {
    if let Some(check) = *MIC_PERMISSION_CHECK
        .lock()
        .expect("mic permission seam lock should not be poisoned")
    {
        return check();
    }
    default_mic_permission_check()
}

#[derive(Debug)]
pub(crate) enum DialogueStartError {
    Speech(SpeechError),
}

impl fmt::Display for DialogueStartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Speech(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for DialogueStartError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VoiceBarMouseDownAction {
    StartOneShotRecording,
    StartAlwaysListeningShell,
    Ignore,
}

pub(crate) trait CardTaskCancelHook: Send {
    fn cancel_all(&mut self);
}

#[cfg(test)]
#[derive(Clone)]
struct TrackingCardTaskCancelHook {
    cancelled: Arc<AtomicBool>,
}

#[cfg(test)]
impl CardTaskCancelHook for TrackingCardTaskCancelHook {
    fn cancel_all(&mut self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }
}

#[derive(Default)]
struct NoopCardTaskCancelHook;

impl CardTaskCancelHook for NoopCardTaskCancelHook {
    fn cancel_all(&mut self) {}
}

pub(crate) struct DialogueControlState {
    session: DialogueSession,
    pcm_stream: Option<PcmStreamSession>,
    card_task_cancel: Box<dyn CardTaskCancelHook>,
    stub_reply_playback: StubReplyPlayback,
    tts_client: Option<TtsClientBox>,
    startup_task: Option<Task<()>>,
    pcm_pump_task: Option<Task<()>>,
    reply_task: Option<Task<()>>,
    next_startup_attempt: u64,
    pending_startup_attempt: Option<u64>,
    error_message: Option<String>,
    #[cfg(test)]
    recording_sink: Option<crate::app::voice::playback::RecordingAssistantAudioSink>,
    #[cfg(test)]
    card_cancel_flag: Option<Arc<AtomicBool>>,
}

impl Default for DialogueControlState {
    fn default() -> Self {
        Self {
            session: DialogueSession::new(),
            pcm_stream: None,
            card_task_cancel: Box::new(NoopCardTaskCancelHook),
            stub_reply_playback: StubReplyPlayback::new(create_assistant_audio_sink()),
            tts_client: None,
            startup_task: None,
            pcm_pump_task: None,
            reply_task: None,
            next_startup_attempt: 0,
            pending_startup_attempt: None,
            error_message: None,
            #[cfg(test)]
            recording_sink: None,
            #[cfg(test)]
            card_cancel_flag: None,
        }
    }
}

impl DialogueControlState {
    pub(crate) fn validate_start_provider(
        values: &VoiceSettingsValues,
    ) -> Result<SherpaProvider, DialogueStartError> {
        match values.paraformer_model_dir() {
            Some(path) => SherpaProvider::new(path),
            None => SherpaProvider::from_env_or_default(),
        }
        .map_err(DialogueStartError::Speech)
    }

    pub(crate) fn phase(&self) -> DialoguePhase {
        self.session.phase()
    }

    #[cfg(test)]
    pub(crate) fn models_resident(&self) -> bool {
        self.session.models_resident()
    }

    pub(crate) fn is_active(&self) -> bool {
        self.pending_startup_attempt.is_some() || self.phase() != DialoguePhase::Idle
    }

    pub(crate) fn dialogue_voice_label(&self) -> Option<String> {
        if let Some(message) = self.error_message.as_ref() {
            return Some(message.clone());
        }
        match self.session.phase() {
            DialoguePhase::Idle => None,
            DialoguePhase::Listening => Some("Always Listening".to_string()),
            DialoguePhase::Thinking => Some("Thinking...".to_string()),
            DialoguePhase::Speaking => Some("Replying...".to_string()),
        }
    }

    pub(crate) fn effective_voice_label(&self, fallback: impl Into<String>) -> String {
        self.dialogue_voice_label()
            .unwrap_or_else(|| fallback.into())
    }

    pub(crate) fn dialogue_voice_visual(&self) -> Option<VoiceVisual> {
        if self.error_message.is_some() {
            return Some(VoiceVisual::Error);
        }
        match self.session.phase() {
            DialoguePhase::Idle => None,
            DialoguePhase::Listening => Some(VoiceVisual::Recording),
            DialoguePhase::Thinking | DialoguePhase::Speaking => Some(VoiceVisual::Processing),
        }
    }

    pub(crate) fn effective_voice_visual(&self, fallback: VoiceVisual) -> VoiceVisual {
        self.dialogue_voice_visual().unwrap_or(fallback)
    }

    pub(crate) fn voice_bar_mouse_down_action(
        &self,
        always_listening_enabled: bool,
    ) -> VoiceBarMouseDownAction {
        if self.is_active() {
            return VoiceBarMouseDownAction::Ignore;
        }
        if always_listening_enabled {
            VoiceBarMouseDownAction::StartAlwaysListeningShell
        } else {
            VoiceBarMouseDownAction::StartOneShotRecording
        }
    }

    pub(crate) fn begin_startup_attempt(&mut self) -> u64 {
        self.next_startup_attempt = self.next_startup_attempt.wrapping_add(1);
        let attempt = self.next_startup_attempt;
        self.pending_startup_attempt = Some(attempt);
        attempt
    }

    pub(crate) fn should_accept_startup_attempt(&self, attempt: u64) -> bool {
        self.pending_startup_attempt == Some(attempt)
    }

    pub(crate) fn finish_startup_attempt(&mut self, attempt: u64) {
        self.startup_task = None;
        if self.pending_startup_attempt == Some(attempt) {
            self.pending_startup_attempt = None;
        }
    }

    pub(crate) fn set_startup_task(&mut self, task: Task<()>) {
        self.startup_task = Some(task);
    }

    pub(crate) fn install_listening_runtime(
        &mut self,
        runtime: chat::DialogueListeningRuntime,
        cx: &mut Context<crate::app::MultisoulApp>,
    ) {
        self.error_message = None;
        self.session.on_listening_started();
        self.pcm_stream = Some(runtime.pcm_stream);
        self.start_pipeline_event_pump(runtime.pipeline_events, cx);
    }

    pub(crate) fn on_final_transcript(&mut self, text: &str) -> Result<(), SpeechError> {
        apply_asr_final(&mut self.session, text)
            .map_err(|error| SpeechError::Recognizer(error.to_string()))
    }

    #[cfg(test)]
    pub(crate) fn pending_transcript(&self) -> Option<&str> {
        self.session.pending_transcript()
    }

    pub(crate) fn mark_start_error(&mut self, error: DialogueStartError) {
        self.stop_pcm_stream();
        self.session.on_listening_stopped();
        self.error_message = Some(start_error_label(&error));
    }

    pub(crate) fn stop_shell(&mut self) {
        self.card_task_cancel.cancel_all();
        self.stub_reply_playback.interrupt();
        self.reply_task = None;
        self.stop_pcm_stream();
        self.session.on_listening_stopped();
        self.error_message = None;
        self.startup_task = None;
        self.pcm_pump_task = None;
        self.pending_startup_attempt = None;
    }

    pub(crate) fn on_window_closed(&mut self) {
        self.stop_shell();
        self.session.on_window_closed();
    }

    #[cfg(test)]
    pub(crate) fn reset_test_seams_for_tests() {
        reply::set_tts_client_factory_for_tests(None);
        reply::set_audio_sink_factory_for_tests(None);
        Self::set_mic_permission_check_for_tests(None);
    }

    #[cfg(test)]
    pub(crate) fn set_mic_permission_check_for_tests(check: Option<MicPermissionCheck>) {
        *MIC_PERMISSION_CHECK
            .lock()
            .expect("mic permission seam lock should not be poisoned") = check;
    }

    #[cfg(test)]
    pub(crate) fn begin_listening_sync_for_tests(
        &mut self,
        values: &VoiceSettingsValues,
    ) -> Result<(), DialogueStartError> {
        if let Err(speech_error) = check_mic_permission() {
            let error = DialogueStartError::Speech(speech_error);
            self.error_message = Some(start_error_label(&error));
            self.stop_pcm_stream();
            self.session.on_listening_stopped();
            return Err(error);
        }
        if let Err(error) = Self::validate_start_provider(values) {
            self.error_message = Some(start_error_label(&error));
            self.stop_pcm_stream();
            self.session.on_listening_stopped();
            return Err(error);
        }
        self.error_message = None;
        self.session.on_listening_started();
        Ok(())
    }

    fn start_pipeline_event_pump(
        &mut self,
        pipeline_events: async_channel::Receiver<ListeningPipelineEvent>,
        cx: &mut Context<crate::app::MultisoulApp>,
    ) {
        let task = cx.spawn(async move |this, cx| {
            while let Ok(event) = pipeline_events.recv().await {
                match event {
                    ListeningPipelineEvent::PartialTranscript(_partial) => {}
                    ListeningPipelineEvent::SpeechStart => {
                        let should_notify = this
                            .update(cx, |app, _cx| app.dialogue.on_speech_start_barge_in())
                            .unwrap_or(false);
                        if should_notify {
                            let _ = this.update(cx, |_app, cx| cx.notify());
                        }
                    }
                    ListeningPipelineEvent::FinalTranscript(text) => {
                        let should_notify = this
                            .update(cx, |app, cx| {
                                if app.dialogue.phase() != DialoguePhase::Listening {
                                    return false;
                                }
                                match app.dialogue.on_final_transcript(&text) {
                                    Ok(()) => {
                                        app.dialogue.begin_stub_reply_synthesis(cx);
                                        true
                                    }
                                    Err(error) => {
                                        tracing::warn!(%error, "failed to apply final transcript");
                                        false
                                    }
                                }
                            })
                            .unwrap_or(false);
                        if should_notify {
                            let _ = this.update(cx, |_app, cx| cx.notify());
                        }
                    }
                }
                let still_active = this
                    .update(cx, |app, _cx| app.dialogue.is_active())
                    .unwrap_or(false);
                if !still_active {
                    break;
                }
            }
        });
        self.pcm_pump_task = Some(task);
    }

    fn begin_stub_reply_synthesis(&mut self, cx: &mut Context<crate::app::MultisoulApp>) {
        if self.session.phase() != DialoguePhase::Thinking {
            return;
        }
        let task = cx.spawn(async move |this, cx| {
            let synthesis_result = this.update(cx, |app, _cx| {
                if app.dialogue.tts_client.is_none() {
                    app.dialogue.tts_client = Some(create_tts_client()?);
                }
                let tts = app
                    .dialogue
                    .tts_client
                    .as_mut()
                    .expect("tts client should exist after lazy init");
                apply_stub_reply_synthesis(
                    &mut app.dialogue.session,
                    tts.as_mut(),
                    &mut app.dialogue.stub_reply_playback,
                )
            });
            let Ok(result) = synthesis_result else {
                return;
            };
            match result {
                Ok(()) => {
                    let _ = this.update(cx, |app, cx| {
                        finish_stub_reply_playback(&mut app.dialogue.session, true);
                        app.dialogue.reply_task = None;
                        cx.notify();
                    });
                }
                Err(error) => {
                    tracing::warn!(%error, "stub reply synthesis failed");
                    let _ = this.update(cx, |app, cx| {
                        app.dialogue.error_message = Some(format!("Speech error: {error}"));
                        app.dialogue.reply_task = None;
                        cx.notify();
                    });
                }
            }
        });
        self.reply_task = Some(task);
    }

    fn on_speech_start_barge_in(&mut self) -> bool {
        if self.session.phase() != DialoguePhase::Speaking {
            return false;
        }
        self.stub_reply_playback.interrupt();
        handle_barge_in_speech_start(&mut self.session);
        self.reply_task = None;
        true
    }

    #[cfg(test)]
    pub(crate) fn prepare_reply_test_fixtures(&mut self) {
        let sink = crate::app::voice::playback::RecordingAssistantAudioSink::new();
        self.recording_sink = Some(sink.clone());
        self.stub_reply_playback = StubReplyPlayback::new(Box::new(sink));
        reply::set_tts_client_factory_for_tests(Some(|| Ok(reply::fake_tts_client_for_tests())));
        let flag = Arc::new(AtomicBool::new(false));
        self.card_cancel_flag = Some(Arc::clone(&flag));
        self.card_task_cancel = Box::new(TrackingCardTaskCancelHook { cancelled: flag });
    }

    #[cfg(test)]
    pub(crate) fn assistant_audio_sink_for_tests(
        &self,
    ) -> crate::app::voice::playback::RecordingAssistantAudioSink {
        self.recording_sink
            .clone()
            .expect("test recording sink should be installed")
    }

    #[cfg(test)]
    pub(crate) fn on_stub_reply_ready_for_tests(&mut self) -> Result<(), SpeechError> {
        if self.tts_client.is_none() {
            self.tts_client = Some(create_tts_client()?);
        }
        let tts = self
            .tts_client
            .as_mut()
            .expect("tts client should exist after lazy init");
        apply_stub_reply_synthesis(
            &mut self.session,
            tts.as_mut(),
            &mut self.stub_reply_playback,
        )
    }

    #[cfg(test)]
    pub(crate) fn on_speech_start_barge_in_for_tests(&mut self) -> bool {
        self.on_speech_start_barge_in()
    }

    #[cfg(test)]
    pub(crate) fn card_tasks_were_cancelled_for_tests(&self) -> bool {
        self.card_cancel_flag
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::SeqCst))
    }

    fn stop_pcm_stream(&mut self) {
        if let Some(stream) = self.pcm_stream.take() {
            stream.cancel();
        }
    }
}

fn start_error_label(error: &DialogueStartError) -> String {
    match error {
        DialogueStartError::Speech(SpeechError::MicrophoneDenied) => {
            "Microphone permission denied".to_string()
        }
        DialogueStartError::Speech(SpeechError::MicrophoneRestricted) => {
            "Microphone permission restricted".to_string()
        }
        DialogueStartError::Speech(SpeechError::ModelNotFound(path)) => {
            format!("Speech model not found: {path}")
        }
        DialogueStartError::Speech(error) => format!("Speech error: {error}"),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/app_dialogue_shell_tests.rs"]
mod app_dialogue_shell_tests;
