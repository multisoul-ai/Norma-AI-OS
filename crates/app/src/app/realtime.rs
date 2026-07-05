//! App-layer shell state for Always Listening realtime voice.

use crate::app::voice::control::VoiceVisual;
use crate::app::MultisoulApp;
use crate::settings::voice::VoiceSettingsValues;
use gpui::Context;
use realtime::{
    RealtimeSessionConfig, RealtimeSessionError, RealtimeSessionId, RealtimeSessionState,
    RealtimeStatus,
};
use std::fmt;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VoiceBarMouseDownAction {
    StartOneShotRecording,
    StartAlwaysListeningShell,
    Ignore,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct RealtimeControlState {
    session: RealtimeSessionState,
    config_error: Option<RealtimeConfigError>,
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

    pub(crate) fn start_shell_with_config(
        &mut self,
        config: RealtimeSessionConfig,
    ) -> Result<(), RealtimeSessionError> {
        self.session.start_connecting(config.session_id)?;
        self.config_error = None;
        Ok(())
    }

    pub(crate) fn mark_started(&mut self) {
        self.session.mark_started();
    }

    pub(crate) fn mark_config_error(&mut self, error: RealtimeConfigError) {
        self.session.mark_error(error.to_string());
        self.config_error = Some(error);
    }

    pub(crate) fn stop_shell(&mut self) {
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
                if self.realtime.start_shell_with_config(config).is_ok() {
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

    pub(crate) fn stop_always_listening_shell(&mut self, cx: &mut Context<Self>) {
        self.realtime.stop_shell();
        cx.notify();
    }
}

#[cfg(test)]
#[path = "../../tests/unit/app_realtime_tests.rs"]
mod tests;
