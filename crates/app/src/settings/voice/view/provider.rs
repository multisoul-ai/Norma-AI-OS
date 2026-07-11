use super::{
    action_button, setting_row, settings_group, text_setting_row, with_alpha, VoiceSettingsInputs,
};
use crate::app::MultisoulApp;
use crate::settings::voice::state::{AgentProviderKind, VoiceSettingsField, VoiceSettingsState};
use gpui::{div, prelude::*, px, rgba, ClickEvent, Context, Div, Window};
use ui::Theme;

pub(super) fn agent_provider_group(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    settings_group(theme, "Agent Provider")
        .child(provider_selector(theme, state, cx))
        .child(text_setting_row(
            theme,
            inputs,
            VoiceSettingsField::AgentModel,
            "Model",
            "Model used by Dialogue and Card Subagent",
            cx,
        ))
}

pub(super) fn agent_provider_advanced_rows(
    theme: Theme,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(text_setting_row(
            theme,
            inputs,
            VoiceSettingsField::AgentBaseUrl,
            "Base URL",
            "Base URL for the selected Ollama or OpenAI-compatible provider",
            cx,
        ))
        .child(api_key_row(theme, inputs, cx))
}

impl MultisoulApp {
    pub(crate) fn on_voice_settings_select_ollama(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings
            .select_agent_provider(AgentProviderKind::Ollama);
        self.voice_settings_inputs
            .sync_from_state(&self.voice_settings, cx);
        self.persist_voice_settings_in_background(cx);
        cx.notify();
    }

    pub(crate) fn on_voice_settings_select_openai(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings
            .select_agent_provider(AgentProviderKind::OpenAiCompatible);
        self.voice_settings_inputs
            .sync_from_state(&self.voice_settings, cx);
        self.persist_voice_settings_in_background(cx);
        cx.notify();
    }

    pub(crate) fn on_voice_settings_clear_api_key(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.clear_agent_api_key();
        self.voice_settings_inputs
            .sync_from_state(&self.voice_settings, cx);
        self.persist_voice_settings_in_background(cx);
        cx.notify();
    }
}

fn provider_selector(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let active = state.values().agent_provider();
    setting_row(
        theme,
        "Provider",
        "One shared provider is used by Dialogue and Card Subagent",
    )
    .child(
        div()
            .w_full()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(8.0))
            .child(
                provider_button(theme, "Ollama", active == AgentProviderKind::Ollama)
                    .id("settings-agent-provider-ollama")
                    .debug_selector(|| "settings-agent-provider-ollama".to_string())
                    .on_click(cx.listener(MultisoulApp::on_voice_settings_select_ollama)),
            )
            .child(
                provider_button(
                    theme,
                    "OpenAI",
                    active == AgentProviderKind::OpenAiCompatible,
                )
                .id("settings-agent-provider-openai")
                .debug_selector(|| "settings-agent-provider-openai".to_string())
                .on_click(cx.listener(MultisoulApp::on_voice_settings_select_openai)),
            ),
    )
}

fn provider_button(theme: Theme, label: &'static str, active: bool) -> Div {
    let accent = if active {
        theme.accent_orange
    } else {
        theme.edge_highlight
    };
    action_button(theme, label, accent).bg(rgba(with_alpha(
        if active {
            theme.accent_orange
        } else {
            theme.panel_high
        },
        if active { 0x28 } else { 0x98 },
    )))
}

fn api_key_row(theme: Theme, inputs: &VoiceSettingsInputs, cx: &mut Context<MultisoulApp>) -> Div {
    setting_row(
        theme,
        "API Key",
        "Stored in app settings and always masked in UI and logs; optional for local providers",
    )
    .child(
        div()
            .w_full()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(px(10.0))
            .child(inputs.input(VoiceSettingsField::AgentApiKey))
            .child(
                action_button(theme, "Clear", theme.destructive)
                    .id("settings-agent-provider-api-key-clear")
                    .debug_selector(|| "settings-agent-provider-api-key-clear".to_string())
                    .on_click(cx.listener(MultisoulApp::on_voice_settings_clear_api_key)),
            ),
    )
}
