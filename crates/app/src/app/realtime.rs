//! App-layer shell state for Always Listening realtime voice.

mod callbacks;
mod card_tasks;
mod chat;
mod shell;

use self::chat::{RealtimePcmStream, RealtimeSidecarClient};
use crate::app::voice::control::VoiceVisual;
use crate::app::voice::playback::{default_assistant_audio_sink, AssistantAudioSink};
use crate::settings::voice::VoiceSettingsValues;
use gpui::Task;
use realtime::{
    RealtimeCommand, RealtimeSessionConfig, RealtimeSessionError, RealtimeSessionId,
    RealtimeSessionState, RealtimeSidecarError, RealtimeStatus, SecretString, SidecarLaunchConfig,
};
use std::fmt;
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

static ACTIVE_REALTIME_SESSION: Mutex<Option<RealtimeSessionId>> = Mutex::new(None);
#[cfg(test)]
pub(crate) static REALTIME_PROCESS_TEST_LOCK: Mutex<()> = Mutex::new(());
const SIDECAR_POLL_INTERVAL: Duration = Duration::from_millis(10);
const SIDECAR_STOP_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Clone, Eq, PartialEq)]
pub(crate) enum RealtimeConfigError {
    MissingApiKey,
    MissingRealtimeModel,
    MissingCardPlannerModel,
    MissingSidecarPath,
}

impl fmt::Debug for RealtimeConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingApiKey => f.write_str("MissingApiKey"),
            Self::MissingRealtimeModel => f.write_str("MissingRealtimeModel"),
            Self::MissingCardPlannerModel => f.write_str("MissingCardPlannerModel"),
            Self::MissingSidecarPath => f.write_str("MissingSidecarPath"),
        }
    }
}

impl fmt::Display for RealtimeConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingApiKey => f.write_str("OpenAI API key is required"),
            Self::MissingRealtimeModel => f.write_str("Realtime model is required"),
            Self::MissingCardPlannerModel => f.write_str("Card planner model is required"),
            Self::MissingSidecarPath => f.write_str("Realtime sidecar path is required"),
        }
    }
}

impl std::error::Error for RealtimeConfigError {}

#[derive(Debug)]
pub(crate) enum RealtimeStartError {
    #[cfg(test)]
    Session(RealtimeSessionError),
    Sidecar(RealtimeSidecarError),
    Speech(voice_sherpa::SpeechError),
}

impl fmt::Display for RealtimeStartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(test)]
            Self::Session(error) => write!(f, "{error}"),
            Self::Sidecar(error) => write!(f, "{error}"),
            Self::Speech(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for RealtimeStartError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VoiceBarMouseDownAction {
    StartOneShotRecording,
    StartAlwaysListeningShell,
    Ignore,
}

pub(crate) struct RealtimeControlState {
    session: RealtimeSessionState,
    config_error: Option<RealtimeConfigError>,
    sidecar: Option<Box<dyn RealtimeSidecarClient>>,
    pcm_stream: Option<Box<dyn RealtimePcmStream>>,
    assistant_audio_sink: Box<dyn AssistantAudioSink>,
    card_tasks: card_tasks::RealtimeCardTaskState,
    startup_task: Option<Task<()>>,
    sidecar_drain_task: Option<Task<()>>,
    pcm_forward_task: Option<Task<()>>,
    next_startup_attempt: u64,
    pending_startup_attempt: Option<u64>,
}

impl Default for RealtimeControlState {
    fn default() -> Self {
        Self {
            session: RealtimeSessionState::default(),
            config_error: None,
            sidecar: None,
            pcm_stream: None,
            assistant_audio_sink: default_assistant_audio_sink(),
            card_tasks: card_tasks::RealtimeCardTaskState::default(),
            startup_task: None,
            sidecar_drain_task: None,
            pcm_forward_task: None,
            next_startup_attempt: 0,
            pending_startup_attempt: None,
        }
    }
}

impl RealtimeControlState {
    pub(crate) fn validate_start_config(
        values: &VoiceSettingsValues,
    ) -> Result<RealtimeSessionConfig, RealtimeConfigError> {
        let Some(_api_key) = values
            .openai_api_key()
            .filter(|value| !value.trim().is_empty())
        else {
            return Err(RealtimeConfigError::MissingApiKey);
        };
        let Some(model) = values
            .realtime_model()
            .filter(|value| !value.trim().is_empty())
        else {
            return Err(RealtimeConfigError::MissingRealtimeModel);
        };
        if values
            .card_planner_model()
            .filter(|value| !value.trim().is_empty())
            .is_none()
        {
            return Err(RealtimeConfigError::MissingCardPlannerModel);
        }
        if values.sidecar_path().is_none() {
            return Err(RealtimeConfigError::MissingSidecarPath);
        }

        Ok(RealtimeSessionConfig {
            session_id: RealtimeSessionId::new("always-listening-shell"),
            model: model.to_string(),
            tools: Vec::new(),
        })
    }

    pub(crate) fn sidecar_launch_config(
        values: &VoiceSettingsValues,
    ) -> Option<SidecarLaunchConfig> {
        let path = values.sidecar_path()?.to_path_buf();
        let openai_api_key = values.openai_api_key()?;
        let (executable, args) = if path.extension().and_then(|ext| ext.to_str()) == Some("mjs") {
            (node_executable(), vec![path.to_string_lossy().to_string()])
        } else {
            (path, Vec::new())
        };

        Some(SidecarLaunchConfig {
            executable,
            args,
            openai_api_key: SecretString::new(openai_api_key.to_string()),
        })
    }

    pub(crate) fn start_shell_with_config(
        &mut self,
        config: RealtimeSessionConfig,
    ) -> Result<(), RealtimeSessionError> {
        if self.is_active() {
            return self.session.start_connecting(config.session_id);
        }
        let session_id = config.session_id.clone();
        acquire_process_slot(&session_id)?;
        if let Err(error) = self.session.start_connecting(config.session_id) {
            release_process_slot(&session_id);
            return Err(error);
        }
        self.config_error = None;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn start_shell_with_sidecar_config(
        &mut self,
        config: RealtimeSessionConfig,
        sidecar_config: SidecarLaunchConfig,
    ) -> Result<(), RealtimeStartError> {
        if self.is_active() {
            return self
                .session
                .start_connecting(config.session_id)
                .map_err(RealtimeStartError::Session);
        }

        let session_id = config.session_id.clone();
        acquire_process_slot(&session_id).map_err(RealtimeStartError::Session)?;
        if let Err(error) = self.session.start_connecting(config.session_id.clone()) {
            release_process_slot(&session_id);
            return Err(RealtimeStartError::Session(error));
        }
        self.config_error = None;

        match chat::spawn_and_start_sidecar(config, sidecar_config) {
            Ok(sidecar) => {
                self.sidecar = Some(sidecar);
                Ok(())
            }
            Err(error) => {
                release_process_slot(&session_id);
                self.session.mark_error(error.to_string());
                Err(RealtimeStartError::Sidecar(error))
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn mark_started(&mut self) {
        self.session.mark_started();
    }

    pub(crate) fn mark_config_error(&mut self, error: RealtimeConfigError) {
        if let Some(session_id) = self.session.session_id().cloned() {
            release_process_slot(&session_id);
        }
        self.stop_chat_runtime();
        self.stop_sidecar_process();
        self.session.mark_error(error.to_string());
        self.config_error = Some(error);
    }

    #[cfg(test)]
    pub(crate) fn drain_sidecar_events(&mut self) -> bool {
        let mut changed = false;
        loop {
            let event = match self.take_sidecar_event() {
                Some(event) => event,
                None => break,
            };

            changed = true;
            match event {
                Ok(event) => {
                    let terminal = matches!(
                        event,
                        realtime::RealtimeEvent::SessionError { .. }
                            | realtime::RealtimeEvent::SessionStopped { .. }
                    );
                    let update = self.apply_sidecar_event(event);
                    self.dispatch_tool_result_callbacks(update);
                    if terminal {
                        break;
                    }
                }
                Err(error) => {
                    self.mark_sidecar_error(error.to_string());
                    break;
                }
            }
        }
        changed
    }

    pub(crate) fn stop_shell(&mut self) {
        let session_id = self.session.session_id().cloned();
        self.stop_chat_runtime();
        if let Some(session_id) = &session_id {
            self.enqueue_stop_session(session_id);
        }
        self.stop_sidecar_process();
        if let Some(session_id) = self.session.session_id().cloned() {
            release_process_slot(&session_id);
        }
        self.reset_after_shutdown();
    }

    pub(crate) fn current_session_id(&self) -> Option<&RealtimeSessionId> {
        self.session.session_id()
    }

    pub(crate) fn begin_chat_startup_attempt(&mut self) -> u64 {
        self.next_startup_attempt = self.next_startup_attempt.wrapping_add(1);
        let attempt = self.next_startup_attempt;
        self.pending_startup_attempt = Some(attempt);
        attempt
    }

    pub(crate) fn should_accept_chat_startup_attempt(
        &self,
        attempt: u64,
        session_id: &RealtimeSessionId,
    ) -> bool {
        self.pending_startup_attempt == Some(attempt)
            && self.current_session_id() == Some(session_id)
            && self.is_active()
    }

    pub(crate) fn finish_chat_startup_attempt(&mut self, attempt: u64) {
        self.startup_task = None;
        if self.pending_startup_attempt == Some(attempt) {
            self.pending_startup_attempt = None;
        }
    }

    pub(crate) fn is_active(&self) -> bool {
        matches!(
            self.session.status(),
            RealtimeStatus::Connecting
                | RealtimeStatus::Listening
                | RealtimeStatus::UserSpeaking
                | RealtimeStatus::AssistantSpeaking
        )
    }

    pub(crate) fn has_sidecar(&self) -> bool {
        self.sidecar.is_some()
    }

    pub(crate) fn realtime_voice_label(&self) -> Option<String> {
        match self.session.status() {
            RealtimeStatus::Off => None,
            RealtimeStatus::Connecting => Some("Connecting...".to_string()),
            RealtimeStatus::AssistantSpeaking => Some("Replying...".to_string()),
            RealtimeStatus::Listening | RealtimeStatus::UserSpeaking => {
                Some("Always Listening".to_string())
            }
            RealtimeStatus::Error => Some("Realtime error".to_string()),
        }
    }

    pub(crate) fn effective_voice_label(&self, fallback: impl Into<String>) -> String {
        self.realtime_voice_label()
            .unwrap_or_else(|| fallback.into())
    }

    pub(crate) fn realtime_voice_visual(&self) -> Option<VoiceVisual> {
        match self.session.status() {
            RealtimeStatus::Off => None,
            RealtimeStatus::Connecting => Some(VoiceVisual::Processing),
            RealtimeStatus::Listening
            | RealtimeStatus::UserSpeaking
            | RealtimeStatus::AssistantSpeaking => Some(VoiceVisual::Recording),
            RealtimeStatus::Error => Some(VoiceVisual::Error),
        }
    }

    pub(crate) fn effective_voice_visual(&self, fallback: VoiceVisual) -> VoiceVisual {
        self.realtime_voice_visual().unwrap_or(fallback)
    }

    pub(crate) fn voice_bar_mouse_down_action(
        &self,
        realtime_enabled: bool,
    ) -> VoiceBarMouseDownAction {
        if self.is_active() {
            return VoiceBarMouseDownAction::Ignore;
        }
        if realtime_enabled {
            VoiceBarMouseDownAction::StartAlwaysListeningShell
        } else {
            VoiceBarMouseDownAction::StartOneShotRecording
        }
    }

    #[cfg(test)]
    pub(crate) fn reset_process_guard_for_tests() {
        let mut active = ACTIVE_REALTIME_SESSION
            .lock()
            .expect("active realtime session lock should not be poisoned");
        *active = None;
    }

    fn reset_after_shutdown(&mut self) {
        let next_startup_attempt = self.next_startup_attempt;
        *self = Self {
            next_startup_attempt,
            ..Self::default()
        };
    }
}

fn node_executable() -> PathBuf {
    std::env::var_os("NODE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("node"))
}

impl RealtimeControlState {
    fn enqueue_stop_session(&self, session_id: &RealtimeSessionId) {
        let _ = self.send_sidecar_command(RealtimeCommand::StopSession {
            session_id: session_id.clone(),
        });
    }

    fn stop_sidecar_process(&mut self) {
        if let Some(sidecar) = self.sidecar.take() {
            stop_sidecar_process_in_background(sidecar);
        }
    }

    fn mark_sidecar_error(&mut self, message: impl Into<String>) {
        if let Some(session_id) = self.session.session_id().cloned() {
            release_process_slot(&session_id);
        }
        self.stop_chat_runtime();
        self.stop_sidecar_process();
        self.session.mark_error(message);
        self.config_error = None;
    }
}

fn stop_sidecar_process_in_background(sidecar: Box<dyn RealtimeSidecarClient>) {
    thread::spawn(move || {
        let _ = sidecar.stop(SIDECAR_STOP_TIMEOUT);
    });
}

impl RealtimeControlState {
    fn finish_sidecar_stopped(&mut self) {
        self.stop_chat_runtime();
        self.stop_sidecar_process();
        if let Some(session_id) = self.session.session_id().cloned() {
            release_process_slot(&session_id);
        }
        self.reset_after_shutdown();
    }
}

fn acquire_process_slot(session_id: &RealtimeSessionId) -> Result<(), RealtimeSessionError> {
    let mut active = ACTIVE_REALTIME_SESSION
        .lock()
        .expect("active realtime session lock should not be poisoned");
    if let Some(existing_session_id) = active.clone() {
        return Err(RealtimeSessionError::AlreadyActive {
            existing_session_id,
        });
    }
    *active = Some(session_id.clone());
    Ok(())
}

fn release_process_slot(session_id: &RealtimeSessionId) {
    let mut active = ACTIVE_REALTIME_SESSION
        .lock()
        .expect("active realtime session lock should not be poisoned");
    if active.as_ref() == Some(session_id) {
        *active = None;
    }
}

#[cfg(test)]
#[path = "../../tests/unit/app_realtime_tests.rs"]
mod app_realtime_tests;

#[cfg(test)]
#[path = "../../tests/unit/app_realtime_sidecar_tests.rs"]
mod app_realtime_tests_sidecar;

#[cfg(test)]
#[path = "../../tests/unit/app_realtime_chat_tests.rs"]
mod app_realtime_tests_chat;

#[cfg(test)]
#[path = "../../tests/unit/app_realtime_chat_lifecycle_tests.rs"]
mod app_realtime_tests_chat_lifecycle;

#[cfg(test)]
#[path = "../../tests/unit/app_realtime_chat_support.rs"]
mod app_realtime_tests_chat_support;

#[cfg(test)]
#[path = "../../tests/unit/app_realtime_card_task_tests.rs"]
mod app_realtime_card_task_tests;

#[cfg(test)]
#[path = "../../tests/unit/app_realtime_tool_call_tests.rs"]
mod app_realtime_tool_call_tests;

#[cfg(test)]
#[path = "../../tests/unit/app_realtime_callback_tests.rs"]
mod app_realtime_callback_tests;
