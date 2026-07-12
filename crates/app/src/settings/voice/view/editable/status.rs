use super::{VoiceSettingsField, VoiceSettingsInputs, VoiceSettingsState, VoiceTextInput};
use crate::app::MultisoulApp;
use gpui::Context;

pub(super) const CONFIGURED_API_KEY_PLACEHOLDER: &str = "Configured ••••••••";
const UNCONFIGURED_API_KEY_PLACEHOLDER: &str = "Not configured";

pub(super) fn input_placeholder(
    state: &VoiceSettingsState,
    field: VoiceSettingsField,
) -> &'static str {
    match field {
        VoiceSettingsField::AgentBaseUrl => "https://api.example.com/v1",
        VoiceSettingsField::AgentModel => "Model name",
        VoiceSettingsField::AgentApiKey
            if state.values().active_provider_profile().api_key().is_some() =>
        {
            CONFIGURED_API_KEY_PLACEHOLDER
        }
        VoiceSettingsField::AgentApiKey => UNCONFIGURED_API_KEY_PLACEHOLDER,
        _ => unreachable!("only Agent Provider fields use VoiceTextInput"),
    }
}

impl VoiceSettingsInputs {
    pub(super) fn sync_field_from_state(
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
