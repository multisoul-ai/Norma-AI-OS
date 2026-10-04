use super::super::asr_provider::locale_catalog;
use super::super::{setting_row, text_setting_row, with_alpha, VoiceSettingsInputs};
use super::components::{compact_setup_row, setup_action_button, SetupButtonTone};
use super::models::alternative_models_detail;
use crate::app::MultisoulApp;
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
            .child("✓ Authorized")
            .into_any_element()
    } else {
        setup_action_button(
            theme,
            "voice-setup-microphone-action",
            if state == VoiceRequirementState::Failed {
                "Open Microphone Settings"
            } else {
                "Grant Access"
            },
            SetupButtonTone::Accent,
            cx,
            cx.listener(MultisoulApp::on_voice_setup_request_microphone),
        )
        .into_any_element()
    };
    compact_setup_row(
        theme,
        "Microphone permission",
        "Required for voice input",
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
        "Recognition source",
        "Choose where speech is processed",
        div()
            .flex_none()
            .flex()
            .gap(px(8.0))
            .child(setup_action_button(
                theme,
                "voice-setup-asr-source-local",
                if active == asr::AsrProviderId::LocalSherpa {
                    "✓ On-device"
                } else {
                    "On-device"
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
                "voice-setup-asr-source-azure",
                if active == asr::AsrProviderId::CloudAzure {
                    "✓ Azure Speech"
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
                "Use On-device",
                "Switch only after a local model is ready",
                setup_action_button(
                    theme,
                    "voice-setup-asr-use-local",
                    "Use On-device",
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
            "Azure region",
            "Where your Speech resource is hosted, for example eastus",
            cx,
        ))
        .child(text_setting_row(
            theme,
            inputs,
            VoiceSettingsField::AzureLocale,
            "Recognition language",
            "Use a supported Azure locale; zh-CN is the default",
            cx,
        ))
        .child(locale_catalog(theme, state, cx))
        .child(
            setting_row(
                theme,
                "Azure API Key",
                "Stored in this Mac's local database; never written to logs",
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
                            "Remove",
                            SetupButtonTone::Destructive,
                            cx,
                            cx.listener(MultisoulApp::on_voice_settings_remove_azure_key),
                        ))
                    }),
            ),
        )
        .child(compact_setup_row(
            theme,
            "Validate configuration",
            "Checks the current region and credential without switching providers",
            setup_action_button(
                theme,
                "settings-asr-azure-validate",
                if profile.is_ready() {
                    "✓ Azure Ready"
                } else {
                    "Validate Azure"
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
                "Test recognition",
                "Speak after the tone; testing never changes the active source",
                setup_action_button(
                    theme,
                    "settings-asr-azure-test-recognition",
                    "Test Azure (3 sec)",
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
                    "Use Azure Speech",
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
                    .text_size(px(12.0))
                    .text_color(rgba(theme.accent_green))
                    .child("Azure Speech is the active recognition source."),
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
