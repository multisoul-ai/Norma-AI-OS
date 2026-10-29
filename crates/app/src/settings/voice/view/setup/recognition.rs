use super::super::asr_provider::locale_catalog;
use super::super::{setting_row, text_setting_row, with_alpha, VoiceSettingsInputs};
use super::components::{compact_setup_row, setup_action_button, SetupButtonTone};
use super::models::alternative_models_detail;
use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use crate::settings::voice::{
    AsrModelInstallState, VoiceRequirementState, VoiceSettingsField, VoiceSettingsState,
    VoiceSetupDestination,
};
use gpui::{div, prelude::*, px, rgba, Context, Div, Stateful};
use ui::Theme;

pub(super) fn recognition_detail(
    theme: Theme,
    microphone: VoiceRequirementState,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let source = state.setup_asr_source();
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(microphone_detail(theme, microphone, cx))
        .child(source_selector(theme, state, source, cx))
        .when(source == asr::AsrProviderId::LocalSherpa, |detail| {
            detail.child(local_detail(theme, state, cx))
        })
        .when(source == asr::AsrProviderId::LocalA1, |detail| {
            detail.child(super::a1::a1_detail(theme, state, cx))
        })
        .when(source == asr::AsrProviderId::CloudAzure, |detail| {
            detail.child(azure_detail(theme, state, inputs, cx))
        })
}

fn microphone_detail(
    theme: Theme,
    state: VoiceRequirementState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let action = if state == VoiceRequirementState::Ready {
        div()
            .id("voice-setup-microphone-action")
            .debug_selector(|| "voice-setup-microphone-action".to_string())
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(8.0))
            .bg(rgba(with_alpha(theme.accent_green, 0x18)))
            .text_color(rgba(theme.text_primary))
            .child(ui::i18n::text("voice.authorized"))
            .into_any_element()
    } else {
        setup_action_button(
            theme,
            "voice-setup-microphone-action",
            if state == VoiceRequirementState::Failed {
                ui::i18n::text("chrome.open_microphone_settings")
            } else {
                ui::i18n::text("voice.grant_access")
            },
            SetupButtonTone::Accent,
            cx,
            cx.listener(MultisoulApp::on_voice_setup_request_microphone),
        )
        .into_any_element()
    };
    compact_setup_row(
        theme,
        ui::i18n::text("voice.microphone_permission"),
        ui::i18n::text("voice.required_for_voice_input"),
        action,
    )
}

fn source_selector(
    theme: Theme,
    state: &VoiceSettingsState,
    source: asr::AsrProviderId,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let active = state.values().asr_provider();
    compact_setup_row(
        theme,
        ui::i18n::text("voice.recognition_source"),
        ui::i18n::text("voice.choose_where_speech_is_processed"),
        div()
            .flex_none()
            .flex()
            .gap(px(8.0))
            .child(setup_action_button(
                theme,
                "voice-setup-asr-source-local",
                if active == asr::AsrProviderId::LocalSherpa {
                    ui::i18n::text("voice.on_device_selected")
                } else {
                    ui::i18n::text("voice.on_device")
                },
                source_tone(
                    active == asr::AsrProviderId::LocalSherpa,
                    source == asr::AsrProviderId::LocalSherpa,
                ),
                cx,
                cx.listener(MultisoulApp::on_voice_setup_show_local_asr),
            ))
            .child(setup_action_button(
                theme,
                "voice-setup-asr-source-a1",
                ui::i18n::text(if active == asr::AsrProviderId::LocalA1 {
                    "voice.a1_selected"
                } else {
                    "voice.a1_name"
                }),
                source_tone(
                    active == asr::AsrProviderId::LocalA1,
                    source == asr::AsrProviderId::LocalA1,
                ),
                cx,
                cx.listener(MultisoulApp::on_voice_setup_show_a1),
            ))
            .child(setup_action_button(
                theme,
                "voice-setup-asr-source-azure",
                if active == asr::AsrProviderId::CloudAzure {
                    ui::i18n::text("voice.azure_speech")
                } else {
                    "Azure Speech"
                },
                source_tone(
                    active == asr::AsrProviderId::CloudAzure,
                    source == asr::AsrProviderId::CloudAzure,
                ),
                cx,
                cx.listener(MultisoulApp::on_voice_setup_show_azure_asr),
            )),
    )
}

fn source_tone(active: bool, viewed: bool) -> SetupButtonTone {
    if active {
        SetupButtonTone::Success
    } else if viewed {
        SetupButtonTone::Accent
    } else {
        SetupButtonTone::Default
    }
}

fn local_detail(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let local_active = state.values().asr_provider() == asr::AsrProviderId::LocalSherpa;
    let local_ready = state
        .values()
        .asr_model_id()
        .and_then(|id| state.model_manager().installation(id))
        .is_some_and(|install| matches!(install, AsrModelInstallState::Ready));
    div()
        .id("voice-setup-asr-local-detail")
        .debug_selector(|| "voice-setup-asr-local-detail".to_string())
        .w_full()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(alternative_models_detail(theme, state, cx))
        .when(local_ready && !local_active, |detail| {
            detail.child(compact_setup_row(
                theme,
                ui::i18n::text("voice.use_on_device"),
                ui::i18n::text("voice.switch_only_after_a_local_model_is_ready"),
                setup_action_button(
                    theme,
                    "voice-setup-asr-use-local",
                    ui::i18n::text("voice.use_on_device"),
                    SetupButtonTone::Accent,
                    cx,
                    cx.listener(MultisoulApp::on_voice_settings_activate_local_asr),
                ),
            ))
        })
}

fn azure_detail(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let profile = state.values().azure_asr_profile();
    let azure_active = state.values().asr_provider() == asr::AsrProviderId::CloudAzure;
    let credential_configured = profile.credential_ref().is_some();
    div()
        .id("voice-setup-asr-azure-detail")
        .debug_selector(|| "voice-setup-asr-azure-detail".to_string())
        .w_full()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(text_setting_row(
            theme,
            inputs,
            VoiceSettingsField::AzureRegion,
            ui::i18n::text("voice.azure_region"),
            ui::i18n::text("voice.where_your_speech_resource_is_hosted_for_example_eastus"),
            cx,
        ))
        .child(text_setting_row(
            theme,
            inputs,
            VoiceSettingsField::AzureLocale,
            ui::i18n::text("voice.recognition_language"),
            ui::i18n::text("voice.use_a_supported_azure_locale_zh_cn_is_the_default"),
            cx,
        ))
        .child(locale_catalog(theme, state, cx))
        .child(
            setting_row(
                theme,
                ui::i18n::text("voice.azure_api_key"),
                ui::i18n::text("voice.stored_in_this_mac_s_local_database_never_written_to_logs"),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(10.0))
                    .child(inputs.input(VoiceSettingsField::AzureApiKey))
                    .when(credential_configured, |row| {
                        row.child(setup_action_button(
                            theme,
                            "settings-asr-azure-key-remove",
                            ui::i18n::text("common.remove"),
                            SetupButtonTone::Destructive,
                            cx,
                            cx.listener(MultisoulApp::on_voice_settings_remove_azure_key),
                        ))
                    }),
            ),
        )
        .child(compact_setup_row(
            theme,
            ui::i18n::text("voice.validate_configuration"),
            ui::i18n::text(
                "voice.checks_the_current_region_and_credential_without_switching_providers",
            ),
            setup_action_button(
                theme,
                "settings-asr-azure-validate",
                if profile.is_ready() {
                    ui::i18n::text("voice.azure_ready")
                } else {
                    ui::i18n::text("voice.validate_azure")
                },
                if profile.is_ready() {
                    SetupButtonTone::Success
                } else {
                    SetupButtonTone::Accent
                },
                cx,
                cx.listener(MultisoulApp::on_voice_settings_validate_azure_asr),
            ),
        ))
        .when(profile.is_ready(), |detail| {
            detail.child(compact_setup_row(
                theme,
                ui::i18n::text("voice.test_recognition"),
                ui::i18n::text(
                    "voice.speak_after_the_tone_testing_never_changes_the_active_source",
                ),
                setup_action_button(
                    theme,
                    "settings-asr-azure-test-recognition",
                    ui::i18n::text("voice.test_azure_3_sec"),
                    SetupButtonTone::Accent,
                    cx,
                    cx.listener(MultisoulApp::on_voice_settings_test_azure_recognition),
                ),
            ))
        })
        .when(profile.is_ready() && !azure_active, |detail| {
            detail.child(
                setup_action_button(
                    theme,
                    "voice-setup-asr-use-azure",
                    ui::i18n::text("voice.use_azure_speech"),
                    SetupButtonTone::Success,
                    cx,
                    cx.listener(MultisoulApp::on_voice_settings_activate_azure_asr),
                )
                .w_full()
                .flex()
                .justify_center(),
            )
        })
        .when(azure_active, |detail| {
            detail.child(
                div()
                    .settings_font(TextRole::Helper)
                    .text_color(rgba(theme.accent_green))
                    .child(ui::i18n::text(
                        "voice.azure_speech_is_the_active_recognition_source",
                    )),
            )
        })
}

impl MultisoulApp {
    pub(crate) fn on_voice_setup_show_local_asr(
        &mut self,
        _: &gpui::ClickEvent,
        _window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings
            .open_setup_destination(VoiceSetupDestination::AsrOnDevice);
        cx.notify();
    }

    pub(crate) fn on_voice_setup_show_azure_asr(
        &mut self,
        _: &gpui::ClickEvent,
        _window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings
            .open_setup_destination(VoiceSetupDestination::AsrAzure);
        cx.notify();
    }
}
