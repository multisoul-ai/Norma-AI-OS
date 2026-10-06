use super::{
    action_button, setting_row, settings_group, text_setting_row, with_alpha, VoiceSettingsInputs,
};
use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use crate::settings::voice::state::{AgentProviderKind, VoiceSettingsField, VoiceSettingsState};
use crate::settings::voice::ProviderDiscoveryStatus;
use gpui::{div, prelude::*, px, rgba, ClickEvent, Context, Div, Window};
use ui::Theme;

mod api_key;
mod controller;

use self::api_key::api_key_row;

pub(super) fn agent_provider_group(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    settings_group(theme, ui::i18n::text("voice.agent_provider"))
        .child(provider_selector(theme, state, cx))
        .child(text_setting_row(
            theme,
            inputs,
            VoiceSettingsField::Model,
            ui::i18n::text("voice.model"),
            ui::i18n::text("voice.model_used_by_dialogue_and_card_subagent"),
            cx,
        ))
}

pub(super) fn agent_provider_advanced_rows(
    theme: Theme,
    state: &VoiceSettingsState,
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
            VoiceSettingsField::BaseUrl,
            ui::i18n::text("voice.base_url"),
            ui::i18n::text("voice.base_url_for_the_selected_ollama_or_openai_compatible_provider"),
            cx,
        ))
        .child(api_key_row(theme, state, inputs, cx))
}

pub(super) fn agent_provider_setup_group(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let mut group = div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(provider_selector(theme, state, cx))
        .child(text_setting_row(
            theme,
            inputs,
            VoiceSettingsField::BaseUrl,
            ui::i18n::text("voice.base_url"),
            ui::i18n::text("voice.openai_compatible_defaults_to_https_api_openai_com_v1"),
            cx,
        ))
        .child(api_key_row(theme, state, inputs, cx))
        .child(
            setting_row(
                theme,
                ui::i18n::text("voice.connect"),
                ui::i18n::text("voice.verify_this_endpoint_and_load_its_available_model_list"),
            )
            .child(
                action_button(
                    theme,
                    if matches!(
                        state.provider_discovery(),
                        ProviderDiscoveryStatus::Connecting
                    ) {
                        ui::i18n::text("voice.connecting")
                    } else if matches!(
                        state.provider_discovery(),
                        ProviderDiscoveryStatus::ManualRequired(_)
                    ) {
                        ui::i18n::text("voice.validate_manual_model")
                    } else {
                        ui::i18n::text("voice.connect_load_models")
                    },
                    theme.accent_blue,
                )
                .id("voice-setup-provider-connect")
                .debug_selector(|| "voice-setup-provider-connect".to_string())
                .on_click(cx.listener(MultisoulApp::on_voice_setup_connect_provider)),
            ),
        );
    group = match state.provider_discovery() {
        ProviderDiscoveryStatus::Models(models) => {
            group.child(provider_model_choices(theme, models, state, cx))
        }
        ProviderDiscoveryStatus::ManualRequired(error) => group
            .child(
                div()
                    .settings_font(TextRole::Helper)
                    .text_color(rgba(theme.destructive))
                    .child(error.clone()),
            )
            .child(text_setting_row(
                theme,
                inputs,
                VoiceSettingsField::Model,
                ui::i18n::text("voice.model"),
                ui::i18n::text("voice.model_list_unavailable_enter_the_exact_model_id_manually"),
                cx,
            )),
        ProviderDiscoveryStatus::ManualValidated(model) => group.child(
            div()
                .settings_font(TextRole::Helper)
                .text_color(rgba(theme.accent_green))
                .child(ui::i18n::format(
                    "voice.validated_model_model",
                    &[("model", (model).to_string())],
                )),
        ),
        ProviderDiscoveryStatus::Failed(error) => group.child(
            div()
                .settings_font(TextRole::Helper)
                .text_color(rgba(theme.destructive))
                .child(error.clone()),
        ),
        ProviderDiscoveryStatus::Idle | ProviderDiscoveryStatus::Connecting => group,
    };
    group
}

impl MultisoulApp {
    pub(crate) fn validate_saved_voice_provider(&mut self, cx: &mut Context<Self>) {
        if self.voice_settings.credential_persistence_pending() {
            return;
        }
        let provider = self.voice_settings.values().agent_provider();
        let profile = self
            .voice_settings
            .values()
            .active_provider_profile()
            .clone();
        if profile.model().trim().is_empty()
            || (provider == AgentProviderKind::OpenAiCompatible && profile.api_key().is_none())
        {
            return;
        }
        let generation = self.voice_settings.begin_provider_discovery();
        let base_url = profile.base_url().to_string();
        let model = profile.model().to_string();
        let api_key = profile.api_key().map(ToOwned::to_owned);
        let work = cx.background_executor().spawn(async move {
            let models = agent::discover_provider_models(
                provider_protocol(provider),
                &base_url,
                api_key.as_deref(),
            )
            .map_err(ProviderConnectionError::Catalog)?;
            if !models.iter().any(|available| available == &model) {
                return Err(ProviderConnectionError::Validation(ui::i18n::message(
                    "voice.saved_model_model_is_not_available_from_this_provider",
                    &[("model", (model).to_string())],
                )));
            }
            agent::validate_provider_model(agent::ProviderConfig::new(
                provider_protocol(provider),
                base_url,
                model.clone(),
                api_key,
            ))
            .map_err(|error| ProviderConnectionError::Validation(error.into()))?;
            Ok((models, model))
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                if !app
                    .voice_settings
                    .accepts_provider_discovery_generation(generation)
                {
                    return;
                }
                match result {
                    Ok((models, model)) => {
                        app.voice_settings.record_provider_models(models);
                        app.voice_settings
                            .record_manual_provider_model_validated(model);
                        app.complete_voice_setup_if_ready(cx);
                    }
                    Err(ProviderConnectionError::Catalog(error)) => {
                        app.voice_settings.record_provider_catalog_error(error)
                    }
                    Err(ProviderConnectionError::Validation(error)) => {
                        app.voice_settings.record_provider_discovery_failure(error)
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

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

    pub(crate) fn on_voice_setup_connect_provider(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let provider = self.voice_settings.values().agent_provider();
        let profile = self
            .voice_settings
            .values()
            .active_provider_profile()
            .clone();
        if provider == AgentProviderKind::OpenAiCompatible && profile.api_key().is_none() {
            self.voice_settings
                .record_provider_discovery_failure(ui::i18n::LocalizedText::new(
                    "voice.add_an_api_key_before_connecting_to_an_openai_compatible_provider",
                ));
            cx.notify();
            return;
        }
        if self.voice_settings.credential_persistence_pending() {
            self.voice_settings
                .record_provider_discovery_failure(ui::i18n::LocalizedText::new(
                    "voice.wait_for_the_api_key_to_finish_saving_before_connecting",
                ));
            cx.notify();
            return;
        }
        let manual_validation = matches!(
            self.voice_settings.provider_discovery(),
            ProviderDiscoveryStatus::ManualRequired(_)
        );
        if manual_validation && profile.model().trim().is_empty() {
            self.voice_settings.record_provider_catalog_error(
                agent::ProviderCatalogError::ModelListUnavailable(
                    ui::i18n::text("voice.enter_the_exact_model_id_before_validating_it")
                        .to_string(),
                ),
            );
            cx.notify();
            return;
        }
        let generation = self.voice_settings.begin_provider_discovery();
        let base_url = profile.base_url().to_string();
        let model = profile.model().to_string();
        let api_key = profile.api_key().map(ToOwned::to_owned);
        let work = cx.background_executor().spawn(async move {
            if manual_validation {
                agent::validate_provider_model(agent::ProviderConfig::new(
                    provider_protocol(provider),
                    base_url,
                    model.clone(),
                    api_key,
                ))
                .map(|()| ProviderConnectionResult::ManualValidated(model))
                .map_err(|error| ProviderConnectionError::Validation(error.into()))
            } else {
                agent::discover_provider_models(
                    provider_protocol(provider),
                    &base_url,
                    api_key.as_deref(),
                )
                .map(ProviderConnectionResult::Models)
                .map_err(ProviderConnectionError::Catalog)
            }
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                if !app
                    .voice_settings
                    .accepts_provider_discovery_generation(generation)
                {
                    return;
                }
                match result {
                    Ok(ProviderConnectionResult::Models(models)) => {
                        app.voice_settings.record_provider_models(models);
                    }
                    Ok(ProviderConnectionResult::ManualValidated(model)) => {
                        app.voice_settings
                            .record_manual_provider_model_validated(model);
                        app.complete_voice_setup_if_ready(cx);
                    }
                    Err(ProviderConnectionError::Catalog(error)) => {
                        app.voice_settings.record_provider_catalog_error(error)
                    }
                    Err(ProviderConnectionError::Validation(error)) => {
                        app.voice_settings.record_provider_discovery_failure(error)
                    }
                }
                cx.notify();
            });
        })
        .detach();
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

    pub(crate) fn on_voice_settings_toggle_api_key_visibility(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let secret = self
            .voice_settings
            .values()
            .active_provider_profile()
            .api_key()
            .map(ToOwned::to_owned);
        self.voice_settings_inputs
            .toggle_api_key_visibility(secret.as_deref(), cx);
        cx.notify();
    }
}

fn provider_model_choices(
    theme: Theme,
    models: &[String],
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let selected = state.active_agent_model();
    let mut choices = div().w_full().flex().flex_col().gap(px(7.0));
    for (index, model) in models.iter().enumerate() {
        let model_id = model.clone();
        choices = choices.child(
            action_button(
                theme,
                model.clone(),
                if selected == model {
                    theme.accent_green
                } else {
                    theme.accent_blue
                },
            )
            .id(("voice-setup-provider-model", index))
            .debug_selector(move || format!("voice-setup-provider-model-{index}"))
            .on_click(cx.listener(move |app, _, _, cx| {
                app.voice_settings
                    .select_discovered_provider_model(model_id.clone());
                app.voice_settings_inputs
                    .sync_from_state(&app.voice_settings, cx);
                app.persist_voice_settings_in_background(cx);
                app.validate_selected_voice_provider(cx);
                cx.notify();
            })),
        );
    }
    setting_row(
        theme,
        ui::i18n::text("voice.model"),
        ui::i18n::text("voice.select_a_model_returned_by_this_provider_connection"),
    )
    .child(choices)
}

enum ProviderConnectionResult {
    Models(Vec<String>),
    ManualValidated(String),
}

enum ProviderConnectionError {
    Catalog(agent::ProviderCatalogError),
    Validation(ui::i18n::LocalizedText),
}

fn provider_protocol(provider: AgentProviderKind) -> agent::ProviderProtocol {
    match provider {
        AgentProviderKind::Ollama => agent::ProviderProtocol::Ollama,
        AgentProviderKind::OpenAiCompatible => agent::ProviderProtocol::OpenAiCompatible,
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
        ui::i18n::text("voice.provider"),
        ui::i18n::text("voice.one_shared_provider_is_used_by_dialogue_and_card_subagent"),
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
                    ui::i18n::text("voice.openai_compatible"),
                    active == AgentProviderKind::OpenAiCompatible,
                )
                .id("settings-agent-provider-openai")
                .debug_selector(|| "settings-agent-provider-openai".to_string())
                .on_click(cx.listener(MultisoulApp::on_voice_settings_select_openai)),
            ),
    )
}

fn provider_button(theme: Theme, label: &'static str, active: bool) -> Div {
    let text_color = if active {
        theme.accent_orange
    } else {
        theme.text_secondary
    };
    action_button(theme, label, text_color)
        .border_color(rgba(with_alpha(
            if active {
                theme.accent_orange
            } else {
                theme.text_primary
            },
            if active { 0x90 } else { 0x28 },
        )))
        .bg(rgba(with_alpha(
            if active {
                theme.accent_orange
            } else {
                theme.text_primary
            },
            if active { 0x28 } else { 0x06 },
        )))
}
