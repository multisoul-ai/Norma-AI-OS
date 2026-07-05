//! App-layer shell state for Always Listening realtime voice.

use crate::app::voice::control::VoiceVisual;
use crate::app::MultisoulApp;
use crate::settings::voice::VoiceSettingsValues;
use gpui::Context;
use realtime::{
    RealtimeCommand, RealtimeEvent, RealtimeSessionConfig, RealtimeSessionError, RealtimeSessionId,
    RealtimeSessionState, RealtimeSidecarError, RealtimeStatus, SecretString, SidecarLaunchConfig,
    SidecarProcess,
};
use std::fmt;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

static ACTIVE_REALTIME_SESSION: Mutex<Option<RealtimeSessionId>> = Mutex::new(None);
const SIDECAR_POLL_INTERVAL: Duration = Duration::from_millis(10);
const SIDECAR_STOP_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Clone, Eq, PartialEq)]
pub(crate) enum RealtimeConfigError {
    MissingApiKey,
    MissingRealtimeModel,
    MissingCardPlannerModel,
}

impl fmt::Debug for RealtimeConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingApiKey => f.write_str("MissingApiKey"),
            Self::MissingRealtimeModel => f.write_str("MissingRealtimeModel"),
            Self::MissingCardPlannerModel => f.write_str("MissingCardPlannerModel"),
        }
    }
}

impl fmt::Display for RealtimeConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingApiKey => f.write_str("OpenAI API key is required"),
            Self::MissingRealtimeModel => f.write_str("Realtime model is required"),
            Self::MissingCardPlannerModel => f.write_str("Card planner model is required"),
        }
    }
}

impl std::error::Error for RealtimeConfigError {}

#[derive(Debug)]
pub(crate) enum RealtimeStartError {
    Session(RealtimeSessionError),
    Sidecar(RealtimeSidecarError),
}

impl fmt::Display for RealtimeStartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Session(error) => write!(f, "{error}"),
            Self::Sidecar(error) => write!(f, "{error}"),
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

#[derive(Default)]
pub(crate) struct RealtimeControlState {
    session: RealtimeSessionState,
    config_error: Option<RealtimeConfigError>,
    sidecar: Option<SidecarProcess>,
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

        match self.spawn_and_start_sidecar(config, sidecar_config) {
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

    fn spawn_and_start_sidecar(
        &self,
        config: RealtimeSessionConfig,
        sidecar_config: SidecarLaunchConfig,
    ) -> Result<SidecarProcess, RealtimeSidecarError> {
        let mut sidecar = SidecarProcess::spawn(sidecar_config)?;
        if let Err(error) = sidecar.send(&RealtimeCommand::StartSession(config)) {
            let _ = sidecar.stop(SIDECAR_STOP_TIMEOUT);
            return Err(error);
        }
        Ok(sidecar)
    }

    pub(crate) fn mark_started(&mut self) {
        self.session.mark_started();
    }

    pub(crate) fn mark_config_error(&mut self, error: RealtimeConfigError) {
        if let Some(session_id) = self.session.session_id().cloned() {
            release_process_slot(&session_id);
        }
        self.stop_sidecar_process();
        self.session.mark_error(error.to_string());
        self.config_error = Some(error);
    }

    pub(crate) fn drain_sidecar_events(&mut self) -> bool {
        let mut changed = false;
        loop {
            let event = match self.sidecar.as_ref().and_then(SidecarProcess::try_recv) {
                Some(event) => event,
                None => break,
            };

            changed = true;
            match event {
                Ok(RealtimeEvent::SessionStarted { .. }) => {
                    self.session.mark_started();
                }
                Ok(RealtimeEvent::SessionError { message, .. }) => {
                    self.mark_sidecar_error(message);
                    break;
                }
                Ok(RealtimeEvent::SessionStopped { .. }) => {
                    self.stop_shell();
                    break;
                }
                Ok(_) => {}
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
        if let Some(session_id) = &session_id {
            self.send_sidecar_stop(session_id);
        }
        self.stop_sidecar_process();
        if let Some(session_id) = self.session.session_id().cloned() {
            release_process_slot(&session_id);
        }
        *self = Self::default();
    }

    pub(crate) fn current_session_id(&self) -> Option<&RealtimeSessionId> {
        self.session.session_id()
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
            RealtimeStatus::Listening
            | RealtimeStatus::UserSpeaking
            | RealtimeStatus::AssistantSpeaking => Some("Always Listening".to_string()),
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
}

fn node_executable() -> PathBuf {
    std::env::var_os("NODE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("node"))
}

impl RealtimeControlState {
    fn send_sidecar_stop(&mut self, session_id: &RealtimeSessionId) {
        if let Some(sidecar) = self.sidecar.as_mut() {
            let _ = sidecar.send(&RealtimeCommand::StopSession {
                session_id: session_id.clone(),
            });
        }
    }

    fn stop_sidecar_process(&mut self) {
        if let Some(sidecar) = self.sidecar.take() {
            let _ = sidecar.stop(SIDECAR_STOP_TIMEOUT);
        }
    }

    fn mark_sidecar_error(&mut self, message: impl Into<String>) {
        if let Some(session_id) = self.session.session_id().cloned() {
            release_process_slot(&session_id);
        }
        self.stop_sidecar_process();
        self.session.mark_error(message);
        self.config_error = None;
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

impl MultisoulApp {
    pub(crate) fn realtime_voice_label(&self) -> Option<String> {
        self.realtime.realtime_voice_label()
    }

    pub(crate) fn effective_voice_label(&self) -> String {
        if self.realtime_voice_label().is_some() {
            self.realtime.effective_voice_label(self.voice_label())
        } else {
            self.voice_label()
        }
    }

    pub(crate) fn effective_voice_visual(&self) -> VoiceVisual {
        self.realtime.effective_voice_visual(self.voice.visual())
    }

    pub(crate) fn start_always_listening_shell(&mut self, cx: &mut Context<Self>) {
        match RealtimeControlState::validate_start_config(self.voice_settings.values()) {
            Ok(config) => {
                let sidecar_config =
                    RealtimeControlState::sidecar_launch_config(self.voice_settings.values());
                if let Some(sidecar_config) = sidecar_config {
                    match self
                        .realtime
                        .start_shell_with_sidecar_config(config, sidecar_config)
                    {
                        Ok(()) => {
                            if let Some(session_id) = self.realtime.current_session_id() {
                                tracing::info!(%session_id, "started always listening sidecar shell");
                            }
                            Self::start_realtime_sidecar_drain_loop(cx);
                        }
                        Err(error) => {
                            tracing::warn!(%error, "failed to start realtime sidecar shell");
                        }
                    }
                } else if self.realtime.start_shell_with_config(config).is_ok() {
                    if let Some(session_id) = self.realtime.current_session_id() {
                        tracing::info!(%session_id, "started always listening shell");
                    }
                    cx.spawn(async move |this, cx| {
                        let _ = this.update(cx, |app, cx| {
                            app.realtime.mark_started();
                            cx.notify();
                        });
                    })
                    .detach();
                }
            }
            Err(error) => self.realtime.mark_config_error(error),
        }
        cx.notify();
    }

    fn start_realtime_sidecar_drain_loop(cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(SIDECAR_POLL_INTERVAL).await;
            let go = this.update(cx, |app, cx| {
                let changed = app.realtime.drain_sidecar_events();
                if changed {
                    cx.notify();
                }
                app.realtime.has_sidecar() && app.realtime.is_active()
            });
            match go {
                Ok(true) => {}
                Ok(false) | Err(_) => break,
            }
        })
        .detach();
    }

    pub(crate) fn stop_always_listening_shell(&mut self, cx: &mut Context<Self>) {
        self.realtime.stop_shell();
        cx.notify();
    }
}

#[cfg(test)]
#[path = "../../tests/unit/app_realtime_tests.rs"]
mod app_realtime_tests;
