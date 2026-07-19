//! App-layer shell state for Continuous Listening local voice dialogue.
mod artifacts;
mod chat;
mod error;
mod error_state;
mod preview;
mod reply;
mod reply_loop;
mod reply_stream;
mod shell;
mod tts_runtime;
use crate::app::voice::control::VoiceVisual;
use crate::app::voice_window_inset::TtsPlaybackEvent;
use crate::settings::voice::{resolve_available_asr_model, VoiceSettingsValues};
pub(crate) use artifacts::VoiceTurnArtifacts;
use dialogue::{
    apply_asr_final, handle_speaking_speech_start, DialoguePhase, DialogueSession,
    SpeakingSpeechSource, UtteranceCommitter,
};
pub(crate) use error::DialogueStartError;
use gpui::{Context, Task};
pub(crate) use preview::{TtsPreviewControl, TtsPreviewState};
#[cfg(test)]
pub(crate) use reply::fake_tts_client_for_tests;
pub(crate) use reply::{
    configure_selected_tts_model, create_tts_client_for_selected, SelectedTtsModel,
};
pub(super) use reply::{
    create_assistant_audio_sink, create_tts_client_for_selection, selected_tts_model,
    selected_tts_model_matches, synthesize_reply, ReplyPlayback,
};
use reply_stream::StreamPlaybackBuffer;
use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;
pub(crate) use tts_runtime::SharedTtsRuntime;
use voice_sherpa::{PcmStreamSession, SherpaProvider, SpeechError};
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VoiceShortcutKeyDownAction {
    StartOneShotRecording,
    StartContinuousListening,
    StopContinuousListening,
    InterruptAssistantReply,
}

pub(crate) struct DialogueControlState {
    pub(super) session: DialogueSession,
    pcm_stream: Option<PcmStreamSession>,
    pub(super) reply_playback: ReplyPlayback,
    pub(super) shared_tts_runtime: SharedTtsRuntime,
    startup_task: Option<Task<()>>,
    pub(super) pcm_pump_task: Option<Task<()>>,
    pub(super) reply_task: Option<Task<()>>,
    pub(super) speech_task: Option<Task<()>>,
    next_startup_attempt: u64,
    pending_startup_attempt: Option<u64>,
    error_message: Option<String>,
    microphone_settings_recovery_available: bool,
    queued_template_reports: Vec<reply_loop::QueuedAgentReport>,
    pub(super) last_reply_text: Option<String>,
    pub(super) pending_voice_turn: bool,
    pub(super) utterance_committer: UtteranceCommitter,
    pub(super) speech_queue: VecDeque<String>,
    playback_buffer: StreamPlaybackBuffer,
    pub(super) speech_synthesis_active: bool,
    pub(super) dialogue_stream_finished: bool,
    pub(super) playback_chunks_pending: usize,
    pub(super) playback_started: bool,
    pub(super) reply_generation: u64,
    pcm_capture: reply_stream::capture::TtsPcmCapture,
    tts_configuration_generation: u64,
    pub(super) tts_cancelled: Arc<AtomicBool>,
    preview_control: Option<Arc<TtsPreviewControl>>,
    dialogue_stream_cancellation: Option<Arc<AtomicBool>>,
    pub(super) reply_stream_started_at: Option<Instant>,
    pub(super) last_reply_interrupted: bool,
    reply_keeps_listening: bool,
    playback_events: Vec<TtsPlaybackEvent>,
    #[cfg(test)]
    recording_sink: Option<crate::app::voice::playback::RecordingAssistantAudioSink>,
}

impl Default for DialogueControlState {
    fn default() -> Self {
        Self {
            session: DialogueSession::new(),
            pcm_stream: None,
            reply_playback: ReplyPlayback::new(create_assistant_audio_sink()),
            shared_tts_runtime: SharedTtsRuntime::default(),
            startup_task: None,
            pcm_pump_task: None,
            reply_task: None,
            speech_task: None,
            next_startup_attempt: 0,
            pending_startup_attempt: None,
            error_message: None,
            microphone_settings_recovery_available: false,
            queued_template_reports: Vec::new(),
            last_reply_text: None,
            pending_voice_turn: false,
            utterance_committer: UtteranceCommitter::new(),
            speech_queue: VecDeque::new(),
            playback_buffer: StreamPlaybackBuffer::new(std::time::Duration::from_millis(120)),
            speech_synthesis_active: false,
            dialogue_stream_finished: false,
            playback_chunks_pending: 0,
            playback_started: false,
            reply_generation: 0,
            pcm_capture: reply_stream::capture::TtsPcmCapture::default(),
            tts_configuration_generation: 0,
            tts_cancelled: Arc::new(AtomicBool::new(false)),
            preview_control: None,
            dialogue_stream_cancellation: None,
            reply_stream_started_at: None,
            last_reply_interrupted: false,
            reply_keeps_listening: true,
            playback_events: Vec::new(),
            #[cfg(test)]
            recording_sink: None,
        }
    }
}

impl DialogueControlState {
    pub(crate) fn cancel_tts_for_configuration_change(&mut self) -> SharedTtsRuntime {
        self.tts_configuration_generation = self.tts_configuration_generation.wrapping_add(1);
        self.cancel_tts_preview();
        self.abort_reply_stream(true);
        self.shared_tts_runtime.invalidate()
    }

    pub(crate) fn tts_configuration_generation(&self) -> u64 {
        self.tts_configuration_generation
    }

    pub(crate) fn shared_tts_runtime(&self) -> SharedTtsRuntime {
        self.shared_tts_runtime.clone()
    }

    pub(crate) fn track_tts_preview(&mut self, control: Arc<TtsPreviewControl>) {
        self.cancel_tts_preview();
        self.preview_control = Some(control);
    }

    fn cancel_tts_preview(&mut self) {
        if let Some(control) = self.preview_control.take() {
            control.cancel();
        }
    }
    pub(super) fn take_playback_events(&mut self) -> Vec<TtsPlaybackEvent> {
        std::mem::take(&mut self.playback_events)
    }
    pub(crate) fn validate_start_provider(
        values: &VoiceSettingsValues,
        model_root: &std::path::Path,
    ) -> Result<SherpaProvider, DialogueStartError> {
        resolve_available_asr_model(values, model_root)
            .map(SherpaProvider::new)
            .map_err(DialogueStartError::Speech)
    }

    pub(crate) fn validate_dialogue_start(
        values: &VoiceSettingsValues,
        model_root: &std::path::Path,
    ) -> Result<SherpaProvider, DialogueStartError> {
        let provider = Self::validate_start_provider(values, model_root)?;
        values
            .agent_provider_config()
            .map_err(|error| DialogueStartError::Provider(error.to_string()))?;
        Ok(provider)
    }

    pub(crate) fn phase(&self) -> DialoguePhase {
        self.session.phase()
    }

    pub(crate) fn set_dialogue_stream_cancellation(&mut self, cancellation: Arc<AtomicBool>) {
        self.dialogue_stream_cancellation = Some(cancellation);
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
            DialoguePhase::Listening => Some("Continuous Listening".to_string()),
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

    pub(crate) fn voice_shortcut_key_down_action(
        &self,
        continuous_listening_enabled: bool,
    ) -> VoiceShortcutKeyDownAction {
        if continuous_listening_enabled {
            if self.phase() == DialoguePhase::Speaking {
                return VoiceShortcutKeyDownAction::InterruptAssistantReply;
            }
            return if self.is_active() {
                VoiceShortcutKeyDownAction::StopContinuousListening
            } else {
                VoiceShortcutKeyDownAction::StartContinuousListening
            };
        }
        VoiceShortcutKeyDownAction::StartOneShotRecording
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
        self.microphone_settings_recovery_available = false;
        self.session.on_listening_started();
        self.pcm_stream = Some(runtime.pcm_stream);
        chat::start_pipeline_event_pump(self, runtime.pipeline_events, cx);
    }

    pub(crate) fn on_final_transcript(&mut self, text: &str) -> Result<(), SpeechError> {
        apply_asr_final(&mut self.session, text)
            .map_err(|error| SpeechError::Recognizer(error.to_string()))?;
        self.error_message = None;
        Ok(())
    }

    pub(crate) fn begin_single_voice_command(&mut self, text: &str) -> Result<(), SpeechError> {
        self.stop_shell();
        self.error_message = None;
        self.microphone_settings_recovery_available = false;
        self.reply_keeps_listening = false;
        self.session.on_listening_started();
        self.on_final_transcript(text)?;
        self.mark_voice_turn_pending();
        Ok(())
    }

    pub(crate) fn recover_reply_failure(&mut self, message: impl Into<String>) {
        self.abort_reply_stream(false);
        self.session
            .on_playback_finished_or_interrupted(self.reply_keeps_listening);
        self.error_message = Some(message.into());
        self.microphone_settings_recovery_available = false;
        self.reply_task = None;
    }

    #[cfg(test)]
    pub(crate) fn pending_transcript(&self) -> Option<&str> {
        self.session.pending_transcript()
    }

    pub(crate) fn pending_transcript_or_err(&self) -> Result<&str, dialogue::DialogueError> {
        self.session
            .pending_transcript()
            .ok_or_else(|| dialogue::DialogueError::Message("missing pending_transcript".into()))
    }

    pub(crate) fn stop_shell(&mut self) {
        self.abort_reply_stream(false);
        self.reply_task = None;
        self.stop_pcm_stream();
        self.session.on_listening_stopped();
        self.error_message = None;
        self.microphone_settings_recovery_available = false;
        self.last_reply_text = None;
        self.pending_voice_turn = false;
        self.startup_task = None;
        self.pcm_pump_task = None;
        self.pending_startup_attempt = None;
        self.reply_keeps_listening = true;
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
        model_root: &std::path::Path,
    ) -> Result<(), DialogueStartError> {
        if let Err(speech_error) = check_mic_permission() {
            let error = DialogueStartError::Speech(speech_error);
            self.record_start_error(&error);
            return Err(error);
        }
        if let Err(error) = Self::validate_dialogue_start(values, model_root) {
            self.record_start_error(&error);
            return Err(error);
        }
        self.error_message = None;
        self.microphone_settings_recovery_available = false;
        self.session.on_listening_started();
        Ok(())
    }

    pub(super) fn on_unclassified_speech_start(&mut self) -> bool {
        let interrupted = handle_speaking_speech_start(
            &mut self.session,
            SpeakingSpeechSource::UnclassifiedMicrophone,
        );
        if !interrupted {
            return false;
        }
        self.abort_reply_stream(true);
        self.reply_task = None;
        true
    }

    pub(crate) fn interrupt_assistant_reply_for_user_action(&mut self) -> bool {
        let interrupted =
            handle_speaking_speech_start(&mut self.session, SpeakingSpeechSource::ConfirmedUser);
        if !interrupted {
            return false;
        }
        self.abort_reply_stream(true);
        self.reply_task = None;
        true
    }

    #[cfg(test)]
    pub(crate) fn prepare_reply_test_fixtures(&mut self) {
        let sink = crate::app::voice::playback::RecordingAssistantAudioSink::new();
        self.recording_sink = Some(sink.clone());
        self.reply_playback = ReplyPlayback::new(Box::new(sink));
        reply::set_tts_client_factory_for_tests(Some(|| Ok(reply::fake_tts_client_for_tests())));
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
    pub(crate) fn on_unclassified_speech_start_for_tests(&mut self) -> bool {
        self.on_unclassified_speech_start()
    }

    fn stop_pcm_stream(&mut self) {
        if let Some(stream) = self.pcm_stream.take() {
            stream.cancel();
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/app_dialogue_shell_tests.rs"]
mod app_dialogue_shell_tests;

#[cfg(test)]
#[path = "../../../tests/unit/agent_report_queue_tests.rs"]
mod agent_report_queue_tests;

#[cfg(test)]
#[path = "../../../tests/unit/reply_stream_tests.rs"]
mod reply_stream_tests;

#[cfg(test)]
#[path = "../../../tests/unit/app_dialogue_shortcut_tests.rs"]
mod app_dialogue_shortcut_tests;
