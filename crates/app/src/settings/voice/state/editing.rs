use super::{
    error_status, ok_status, AgentProviderKind, VoiceSettingsField, VoiceSettingsState,
    KEY_AGENT_OLLAMA_BASE_URL, KEY_AGENT_OLLAMA_MODEL, KEY_AGENT_OPENAI_BASE_URL,
    KEY_AGENT_OPENAI_MODEL,
};

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
        self.commit_edit(field)
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
                VoiceSettingsField::ApiKey | VoiceSettingsField::AzureApiKey
            )
        {
            format!("{}|", "•".repeat(self.draft.chars().count()))
        } else if self.editing == Some(field) {
            format!("{}|", self.draft)
        } else if matches!(
            field,
            VoiceSettingsField::ApiKey | VoiceSettingsField::AzureApiKey
        ) {
            if self
                .values
                .active_provider_profile()
                .credential_configured()
            {
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

    fn commit_edit(&mut self, field: VoiceSettingsField) -> Option<String> {
        self.save_edit(field, true)
    }

    fn save_edit(&mut self, field: VoiceSettingsField, finish_editing: bool) -> Option<String> {
        let value = self.draft.trim().to_string();
        if value.is_empty() {
            if matches!(
                field,
                VoiceSettingsField::ApiKey | VoiceSettingsField::AzureApiKey
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
                self.finish_editing(finish_editing);
                self.refresh_values();
                return Some(value);
            }
            VoiceSettingsField::AzureRegion => {
                let locale = self.values.azure_asr_profile().locale().to_string();
                let credential_ref = self
                    .values
                    .azure_asr_profile()
                    .credential_ref()
                    .map(ToOwned::to_owned);
                self.update_azure_asr_draft(&value, &locale, credential_ref);
                self.finish_editing(finish_editing);
                return Some(value);
            }
            VoiceSettingsField::AzureLocale => {
                if asr::validate_azure_speech_locale(&value).is_err() {
                    self.status = Some(error_status("Choose a supported Azure Speech locale"));
                    return None;
                }
                let region = self.values.azure_asr_profile().region().to_string();
                let credential_ref = self
                    .values
                    .azure_asr_profile()
                    .credential_ref()
                    .map(ToOwned::to_owned);
                self.update_azure_asr_draft(&region, &value, credential_ref);
                self.finish_editing(finish_editing);
                return Some(value);
            }
            VoiceSettingsField::AzureApiKey => {
                self.save_azure_asr_credential(value.clone());
                self.finish_editing(finish_editing);
                self.refresh_values();
                return Some(value);
            }
        };
        self.persist_string(key, &value);
        self.invalidate_provider_requirement();
        self.finish_editing(finish_editing);
        self.refresh_values();
        Some(value)
    }

    fn finish_editing(&mut self, finish_editing: bool) {
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
            VoiceSettingsField::ApiKey => String::new(),
            VoiceSettingsField::AzureRegion => self.values.azure_asr_profile().region().to_string(),
            VoiceSettingsField::AzureLocale => self.values.azure_asr_profile().locale().to_string(),
            VoiceSettingsField::AzureApiKey => String::new(),
        }
    }

    pub(crate) fn editor_value(&self, field: VoiceSettingsField) -> String {
        self.field_value(field)
    }

    pub(crate) fn azure_locale_query(&self) -> &str {
        if self.editing == Some(VoiceSettingsField::AzureLocale) {
            self.draft.as_str()
        } else {
            self.values.azure_asr_profile().locale()
        }
    }
}
