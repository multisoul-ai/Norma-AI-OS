pub(super) fn owns_account(account: &str) -> bool {
    account.starts_with("gpt-live-desktop-")
}

/// Credentials stay in the existing credential store; this value contains only its reference.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct GptLiveProfile {
    pub(super) credential_ref: Option<String>,
    pub(super) voice: String,
}

impl Default for GptLiveProfile {
    fn default() -> Self {
        Self::from_stored(None, None)
    }
}

impl std::fmt::Debug for GptLiveProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GptLiveProfile")
            .field(
                "credential_ref",
                &self.credential_ref.as_ref().map(|_| "<opaque>"),
            )
            .field("voice", &self.voice)
            .finish()
    }
}

impl GptLiveProfile {
    pub(crate) fn from_stored(credential_ref: Option<String>, voice: Option<String>) -> Self {
        Self {
            credential_ref: credential_ref.filter(|value| !value.trim().is_empty()),
            voice: voice
                .filter(|value| matches!(value.as_str(), "marin" | "cedar"))
                .unwrap_or_else(|| "marin".into()),
        }
    }

    pub(crate) fn credential_ref(&self) -> Option<&str> {
        self.credential_ref
            .as_deref()
            .map(str::trim)
            .filter(|account| owns_account(account))
    }
    pub(crate) fn voice(&self) -> &str {
        &self.voice
    }
}

impl super::VoiceSettingsState {
    pub(crate) fn gpt_live_key_expanded(&self) -> bool {
        self.gpt_live_key_expanded
    }

    pub(crate) fn set_gpt_live_key_expanded(&mut self, expanded: bool) {
        self.gpt_live_key_expanded = expanded;
    }

    pub(crate) fn gpt_live_diagnostics_expanded(&self) -> bool {
        self.gpt_live_diagnostics_expanded
    }

    pub(crate) fn toggle_gpt_live_diagnostics(&mut self) {
        self.gpt_live_diagnostics_expanded = !self.gpt_live_diagnostics_expanded;
    }

    pub(crate) fn select_gpt_live_interaction_mode(&mut self, mode: super::VoiceInteractionMode) {
        self.saved.gpt_live_interaction_mode = Some(mode);
        self.persist_string(super::values::KEY_GPT_LIVE_INTERACTION_MODE, mode.as_str());
        self.refresh_values();
    }

    pub(crate) fn select_gpt_live_voice(&mut self, voice: &str) -> bool {
        if !matches!(voice, "marin" | "cedar") {
            return false;
        }
        self.saved.gpt_live_profile.voice = voice.into();
        self.persist_string(super::values::KEY_GPT_LIVE_VOICE, voice);
        self.refresh_values();
        true
    }
}

impl super::VoiceSettingsValues {
    pub(crate) fn gpt_live_interaction_mode(&self) -> super::VoiceInteractionMode {
        self.gpt_live_interaction_mode
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/gpt_live_profile_tests.rs"]
mod tests;
