//! App-layer shell state for Always Listening local voice dialogue.

mod chat;
mod shell;

use crate::app::voice::control::VoiceVisual;
use crate::settings::voice::VoiceSettingsValues;
use dialogue::{DialoguePhase, DialogueSession};
use gpui::{Context, Task};
use std::fmt;
use std::sync::Mutex;
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

#[derive(Default)]
struct NoopCardTaskCancelHook;

impl CardTaskCancelHook for NoopCardTaskCancelHook {
    fn cancel_all(&mut self) {}
}

pub(crate) struct DialogueControlState {
    session: DialogueSession,
    pcm_stream: Option<PcmStreamSession>,
    card_task_cancel: Box<dyn CardTaskCancelHook>,
    startup_task: Option<Task<()>>,
    pcm_pump_task: Option<Task<()>>,
    next_startup_attempt: u64,
    pending_startup_attempt: Option<u64>,
    error_message: Option<String>,
}

impl Default for DialogueControlState {
    fn default() -> Self {
        Self {
            session: DialogueSession::new(),
            pcm_stream: None,
            card_task_cancel: Box::new(NoopCardTaskCancelHook),
            startup_task: None,
            pcm_pump_task: None,
            next_startup_attempt: 0,
            pending_startup_attempt: None,
            error_message: None,
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
        self.start_pcm_pump(runtime.pcm_chunks, cx);
    }

    pub(crate) fn mark_start_error(&mut self, error: DialogueStartError) {
        self.stop_pcm_stream();
        self.session.on_listening_stopped();
        self.error_message = Some(start_error_label(&error));
    }

    pub(crate) fn stop_shell(&mut self) {
        self.card_task_cancel.cancel_all();
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

    fn start_pcm_pump(
        &mut self,
        pcm_chunks: async_channel::Receiver<audio::PcmAudioChunk>,
        cx: &mut Context<crate::app::MultisoulApp>,
    ) {
        let task = cx.spawn(async move |this, cx| {
            while let Ok(_chunk) = pcm_chunks.recv().await {
                let go = this.update(cx, |app, _cx| app.dialogue.is_active());
                match go {
                    Ok(true) => {}
                    Ok(false) | Err(_) => break,
                }
            }
        });
        self.pcm_pump_task = Some(task);
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
