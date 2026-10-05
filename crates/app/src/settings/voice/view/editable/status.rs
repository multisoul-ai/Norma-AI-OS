use super::{VoiceSettingsField, VoiceSettingsInputs, VoiceSettingsState, VoiceTextInput};
use crate::app::MultisoulApp;
use gpui::Context;

pub(super) const CONFIGURED_API_KEY_PLACEHOLDER: &str = "*********";
const UNCONFIGURED_API_KEY_PLACEHOLDER: &str = "voice.not_configured";

pub(super) fn input_placeholder(
    state: &VoiceSettingsState,
    field: VoiceSettingsField,
) -> &'static str {
    match field {
        VoiceSettingsField::BaseUrl => "https://api.example.com/v1",
        VoiceSettingsField::Model => ui::i18n::text("voice.model_name"),
        VoiceSettingsField::ApiKey
            if state
                .values()
                .active_provider_profile()
                .credential_configured() =>
        {
            CONFIGURED_API_KEY_PLACEHOLDER
        }
        VoiceSettingsField::AzureApiKey
            if state
                .values()
                .azure_asr_profile()
                .credential_ref()
                .is_some() =>
        {
            CONFIGURED_API_KEY_PLACEHOLDER
        }
        VoiceSettingsField::ApiKey | VoiceSettingsField::AzureApiKey => {
            ui::i18n::text(UNCONFIGURED_API_KEY_PLACEHOLDER)
        }
        VoiceSettingsField::AzureRegion => "eastus",
        VoiceSettingsField::AzureLocale => "zh-CN",
        VoiceSettingsField::MiniMaxApiKey if state.minimax_credential_configured() => {
            CONFIGURED_API_KEY_PLACEHOLDER
        }
        VoiceSettingsField::MiniMaxApiKey => ui::i18n::text(UNCONFIGURED_API_KEY_PLACEHOLDER),
        VoiceSettingsField::DoubaoApiKey if state.values().doubao_credential_ref().is_some() => {
            CONFIGURED_API_KEY_PLACEHOLDER
        }
        VoiceSettingsField::DoubaoApiKey => ui::i18n::text(UNCONFIGURED_API_KEY_PLACEHOLDER),
        VoiceSettingsField::MiniMaxVoiceId => ui::i18n::text("voice.enter_voice_id"),
        VoiceSettingsField::DoubaoVoiceSearch => ui::i18n::text("doubao.search_voices"),
        VoiceSettingsField::MiniMaxVoiceSearch => {
            ui::i18n::text("voice.search_by_name_or_voice_id")
        }
    }
}

impl VoiceSettingsInputs {
    pub(crate) fn sync_field_from_state(
        &self,
        field: VoiceSettingsField,
        state: &VoiceSettingsState,
        cx: &mut Context<MultisoulApp>,
    ) {
        let value = state.editor_value(field);
        let placeholder = input_placeholder(state, field);
        self.input(field).update(cx, |input, cx| {
            input.sync_value_with_placeholder(value, placeholder, cx)
        });
    }
}

impl VoiceTextInput {
    pub(super) fn sync_value_with_placeholder(
        &mut self,
        value: String,
        placeholder: &'static str,
        cx: &mut Context<Self>,
    ) {
        if self.content.as_ref() == value.as_str()
            && self.placeholder.as_ref() == placeholder
            && self.marked_range.is_none()
        {
            self.auto_save_task = None;
            return;
        }
        self.placeholder = placeholder.into();
        self.sync_value(value, cx);
    }
}
