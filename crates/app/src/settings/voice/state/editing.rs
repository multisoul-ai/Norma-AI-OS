use super::provider_values::{
    AgentProviderKind, KEY_AGENT_OLLAMA_BASE_URL, KEY_AGENT_OLLAMA_MODEL,
    KEY_AGENT_OPENAI_BASE_URL, KEY_AGENT_OPENAI_MODEL,
};
use super::{error_status, ok_status, VoiceSettingsField, VoiceSettingsState};
use tts::{TtsModelId, TtsVoiceId};

impl VoiceSettingsState {
    #[cfg(test)]
    pub(crate) fn begin_edit(&mut self, field: VoiceSettingsField) {
        self.draft = self.field_value(field);
        self.editing = Some(field);
        self.status = None;
    }

    pub(crate) fn begin_edit_with_value(&mut self, field: VoiceSettingsField, value: String) {
        self.editing = Some(field);
        self.draft = value;
        self.status = None;
    }

    pub(crate) fn update_edit_value(&mut self, field: VoiceSettingsField, value: String) {
        if self.editing == Some(field) {
            self.draft = value;
        }
    }

    pub(crate) fn auto_save_edit_value(
        &mut self,
        field: VoiceSettingsField,
        value: String,
    ) -> Option<String> {
        if self.editing != Some(field) {
            return None;
        }
        self.draft = value;
        self.save_edit(field, false)
    }

    pub(crate) fn commit_edit_value(
        &mut self,
        field: VoiceSettingsField,
        value: String,
    ) -> Option<String> {
        self.editing = Some(field);
        self.draft = value;
        self.save_edit(field, true)
    }

    pub(crate) fn cancel_edit_field(&mut self, field: VoiceSettingsField) {
        if self.editing == Some(field) {
            self.cancel_edit();
        }
    }

    #[cfg(test)]
    pub(super) fn row_value(&self, field: VoiceSettingsField) -> String {
        if self.editing == Some(field)
            && matches!(
                field,
                VoiceSettingsField::ApiKey | VoiceSettingsField::MiniMaxApiKey
            )
        {
            format!("{}|", "•".repeat(self.draft.chars().count()))
        } else if self.editing == Some(field) {
            format!("{}|", self.draft)
        } else if field == VoiceSettingsField::ApiKey {
            if self
                .values
                .active_provider_profile()
                .credential_configured()
            {
                "*********".to_string()
            } else {
                "Not configured".to_string()
            }
        } else if field == VoiceSettingsField::MiniMaxApiKey {
            if self.values.minimax_tts_credential_ref().is_some() {
                "*********".to_string()
            } else {
                "Not configured".to_string()
            }
        } else {
            self.field_value(field)
        }
    }

    pub(super) fn cancel_edit(&mut self) {
        self.editing = None;
        self.draft.clear();
        self.status = None;
    }

    fn save_edit(&mut self, field: VoiceSettingsField, finish_editing: bool) -> Option<String> {
        let value = self.draft.trim().to_string();
        if field == VoiceSettingsField::MiniMaxVoiceSearch {
            self.minimax_voice_search = value.clone();
            self.finish_editing_if_requested(finish_editing);
            return Some(value);
        }
        if value.is_empty() {
            if matches!(
                field,
                VoiceSettingsField::ApiKey | VoiceSettingsField::MiniMaxApiKey
            ) {
                if finish_editing {
                    self.editing = None;
                    self.draft.clear();
                }
                self.status = Some(ok_status("API Key unchanged; use Clear to remove it"));
            } else {
                self.status = Some(error_status("Value cannot be empty"));
            }
            return None;
        }
        let key = match field {
            VoiceSettingsField::BaseUrl => match self.values.agent_provider() {
                AgentProviderKind::Ollama => {
                    self.saved.ollama_profile.base_url = Some(value.clone());
                    KEY_AGENT_OLLAMA_BASE_URL
                }
                AgentProviderKind::OpenAiCompatible => {
                    self.saved.openai_profile.base_url = Some(value.clone());
                    KEY_AGENT_OPENAI_BASE_URL
                }
            },
            VoiceSettingsField::Model => match self.values.agent_provider() {
                AgentProviderKind::Ollama => {
                    self.saved.ollama_profile.model = Some(value.clone());
                    KEY_AGENT_OLLAMA_MODEL
                }
                AgentProviderKind::OpenAiCompatible => {
                    self.saved.openai_profile.model = Some(value.clone());
                    KEY_AGENT_OPENAI_MODEL
                }
            },
            VoiceSettingsField::ApiKey => {
                self.save_active_provider_credential(value.clone());
                self.finish_editing_if_requested(finish_editing);
                self.refresh_values();
                return Some(value);
            }
            VoiceSettingsField::MiniMaxApiKey => {
                self.update_minimax_api_key_draft(value.clone());
                self.finish_editing_if_requested(finish_editing);
                return Some(value);
            }
            VoiceSettingsField::MiniMaxVoiceId => {
                let voice_id = match TtsVoiceId::parse(value.clone()) {
                    Ok(voice_id) => voice_id,
                    Err(_) => {
                        self.status = Some(error_status("Voice ID cannot be empty"));
                        return None;
                    }
                };
                let model_id = self
                    .values
                    .minimax_tts_model_id()
                    .cloned()
                    .unwrap_or_else(|| {
                        TtsModelId::parse(tts::MINIMAX_SPEECH_TURBO_MODEL_ID)
                            .expect("built-in MiniMax Turbo model ID must be valid")
                    });
                self.set_minimax_tts_selection(
                    model_id,
                    voice_id,
                    tts::VoiceSelectionProvenance::Manual,
                );
                self.finish_editing_if_requested(finish_editing);
                return Some(value);
            }
            VoiceSettingsField::MiniMaxVoiceSearch => {
                unreachable!("MiniMax Voice search is handled before persistent setting validation")
            }
        };
        self.persist_string(key, &value);
        self.invalidate_provider_requirement();
        self.finish_editing_if_requested(finish_editing);
        self.refresh_values();
        Some(value)
    }

    fn finish_editing_if_requested(&mut self, finish_editing: bool) {
        if finish_editing {
            self.editing = None;
            self.draft.clear();
        }
    }

    fn field_value(&self, field: VoiceSettingsField) -> String {
        match field {
            VoiceSettingsField::BaseUrl => {
                self.values.active_provider_profile().base_url().to_string()
            }
            VoiceSettingsField::Model => self.values.active_provider_profile().model().to_string(),
            VoiceSettingsField::ApiKey | VoiceSettingsField::MiniMaxApiKey => String::new(),
            VoiceSettingsField::MiniMaxVoiceId => self
                .values
                .minimax_tts_voice_id()
                .map(TtsVoiceId::as_str)
                .unwrap_or_default()
                .to_string(),
            VoiceSettingsField::MiniMaxVoiceSearch => self.minimax_voice_search.clone(),
        }
    }

    pub(crate) fn minimax_voice_search(&self) -> &str {
        &self.minimax_voice_search
    }

    pub(crate) fn editor_value(&self, field: VoiceSettingsField) -> String {
        self.field_value(field)
    }
}
