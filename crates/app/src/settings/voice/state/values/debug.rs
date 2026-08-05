use super::super::VoiceSettingsField;
use super::{SavedVoiceSettings, VoiceSettingsValues};
use crate::settings::voice::VoiceSettingsState;
use std::fmt;

impl fmt::Debug for SavedVoiceSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SavedVoiceSettings")
            .field("interaction_mode", &self.interaction_mode)
            .field("dictation_trigger", &self.dictation_trigger)
            .field("asr_model_id", &self.asr_model_id)
            .field("asr_provider", &self.asr_provider)
            .field("azure_asr_region", &self.azure_asr_region)
            .field("azure_asr_locale", &self.azure_asr_locale)
            .field(
                "azure_asr_credential_ref",
                &self.azure_asr_credential_ref.as_ref().map(|_| "<opaque>"),
            )
            .field("azure_asr_ready", &self.azure_asr_ready)
            .field("agent_provider", &self.agent_provider)
            .field("ollama_profile", &self.ollama_profile)
            .field("openai_profile_base_url", &self.openai_profile.base_url)
            .field("openai_profile_model", &self.openai_profile.model)
            .field(
                "openai_profile_api_key",
                &self.openai_profile.api_key.as_ref().map(|_| "<redacted>"),
            )
            .field("ollama_endpoint", &self.ollama_endpoint)
            .field("planner_model", &self.planner_model)
            .field("dialogue_model", &self.dialogue_model)
            .field("refine_model", &self.refine_model)
            .field("input_device_id", &self.input_device_id)
            .field("trace_enabled", &self.trace_enabled)
            .field(
                "agent_task_announcements_enabled",
                &self.agent_task_announcements_enabled,
            )
            .field(
                "voice_window_inset_enabled",
                &self.voice_window_inset_enabled,
            )
            .finish()
    }
}

impl fmt::Debug for VoiceSettingsValues {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VoiceSettingsValues")
            .field("interaction_mode", &self.interaction_mode)
            .field("dictation_trigger", &self.dictation_trigger)
            .field("asr_model_id", &self.asr_model_id)
            .field("asr_provider", &self.asr_provider)
            .field("azure_asr_profile", &self.azure_asr_profile)
            .field("agent_provider", &self.agent_provider)
            .field("ollama_profile", &self.ollama_profile)
            .field("openai_profile", &self.openai_profile)
            .field("ollama_endpoint", &self.ollama_endpoint)
            .field("planner_model", &self.planner_model)
            .field("dialogue_model", &self.dialogue_model)
            .field("refine_model", &self.refine_model)
            .field("input_device_id", &self.input_device_id)
            .field("trace_enabled", &self.trace_enabled)
            .field(
                "agent_task_announcements_enabled",
                &self.agent_task_announcements_enabled,
            )
            .field(
                "voice_window_inset_enabled",
                &self.voice_window_inset_enabled,
            )
            .finish()
    }
}

impl fmt::Debug for VoiceSettingsState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let draft = if matches!(
            self.editing,
            Some(
                VoiceSettingsField::ApiKey
                    | VoiceSettingsField::AzureApiKey
                    | VoiceSettingsField::MiniMaxApiKey
            )
        ) {
            "<redacted>"
        } else {
            self.draft.as_str()
        };
        f.debug_struct("VoiceSettingsState")
            .field("values", &self.values)
            .field("saved", &self.saved)
            .field("env", &self.env)
            .field("storage", &self.storage)
            .field("input_devices", &self.input_devices)
            .field("editing", &self.editing)
            .field("draft", &draft)
            .field("status", &self.status)
            .finish()
    }
}
