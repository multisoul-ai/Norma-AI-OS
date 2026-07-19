use super::{action_button, setting_row, text_setting_row, VoiceSettingsInputs};
use crate::app::MultisoulApp;
use crate::settings::voice::{VoiceSettingsField, VoiceSettingsState};
use gpui::{div, prelude::*, px, rgba, Context, Div};
use ui::Theme;

mod controller;

pub(super) fn tts_provider_settings(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let active = state
        .values()
        .active_tts_provider_id()
        .map(tts::TtsProviderId::as_str);
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            setting_row(
                theme,
                "TTS Provider",
                "Choose where spoken text is synthesized; remote providers receive reply text",
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(
                        action_button(
                            theme,
                            if active == Some(tts::LOCAL_TTS_PROVIDER_ID) {
                                "On-device · Active"
                            } else {
                                "Use On-device"
                            },
                            theme.accent_blue,
                        )
                        .id("settings-tts-provider-local")
                        .debug_selector(|| "settings-tts-provider-local".to_string())
                        .on_click(cx.listener(MultisoulApp::on_voice_use_local_tts)),
                    )
                    .child(
                        action_button(
                            theme,
                            if active == Some(tts::MINIMAX_TTS_PROVIDER_ID) {
                                "MiniMax · Active"
                            } else {
                                "Configure MiniMax"
                            },
                            theme.accent_orange,
                        )
                        .id("settings-tts-provider-minimax")
                        .debug_selector(|| "settings-tts-provider-minimax".to_string()),
                    ),
            ),
        )
        .child(minimax_configuration(theme, state, inputs, cx))
}

fn minimax_configuration(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let readiness = state.minimax_readiness();
    let mut group = div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .p(px(12.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(rgba(theme.edge_highlight))
        .child(text_setting_row(
            theme,
            inputs,
            VoiceSettingsField::MiniMaxApiKey,
            "MiniMax API Key",
            "Validated credentials are stored only in macOS Keychain",
            cx,
        ))
        .child(
            setting_row(
                theme,
                "Connection",
                "Connect in the background and load Voices owned by this account",
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        action_button(
                            theme,
                            if state.minimax_account_status() == tts::RemoteAccountStatus::Checking
                            {
                                "Connecting…"
                            } else {
                                "Connect & Load Voices"
                            },
                            theme.accent_blue,
                        )
                        .id("settings-tts-minimax-connect")
                        .debug_selector(|| "settings-tts-minimax-connect".to_string())
                        .on_click(cx.listener(MultisoulApp::on_voice_connect_minimax_tts)),
                    )
                    .when(
                        state.values().minimax_tts_credential_ref().is_some(),
                        |actions| {
                            actions.child(
                                action_button(theme, "Clear Credential", theme.destructive)
                                    .id("settings-tts-minimax-clear-credential")
                                    .debug_selector(|| {
                                        "settings-tts-minimax-clear-credential".to_string()
                                    })
                                    .on_click(cx.listener(
                                        MultisoulApp::on_voice_clear_minimax_tts_credential,
                                    )),
                            )
                        },
                    ),
            ),
        )
        .child(minimax_model_selector(theme, state, cx))
        .child(text_setting_row(
            theme,
            inputs,
            VoiceSettingsField::MiniMaxVoiceId,
            "Voice ID",
            "Select a Voice below or enter any known Voice ID manually",
            cx,
        ));
    if !state.minimax_voice_catalog().voices().is_empty() {
        group = group
            .child(text_setting_row(
                theme,
                inputs,
                VoiceSettingsField::MiniMaxVoiceSearch,
                "Search Voices",
                "Filter the account catalog by display name or exact Voice ID",
                cx,
            ))
            .child(minimax_voice_catalog(theme, state, cx));
    }
    group
        .child(
            div()
                .text_size(px(12.0))
                .text_color(rgba(match readiness {
                    tts::RemoteTtsReadiness::Ready => theme.accent_green,
                    tts::RemoteTtsReadiness::TemporarilyUnavailable
                    | tts::RemoteTtsReadiness::InvalidCredential
                    | tts::RemoteTtsReadiness::VoiceUnavailable => theme.destructive,
                    _ => theme.text_tertiary,
                }))
                .child(readiness_label(readiness)),
        )
        .child(
            action_button(theme, "Preview / Validate", theme.accent_blue)
                .id("settings-tts-minimax-preview")
                .debug_selector(|| "settings-tts-minimax-preview".to_string())
                .on_click(cx.listener(MultisoulApp::on_voice_preview_minimax_tts)),
        )
        .child(
            action_button(
                theme,
                if readiness == tts::RemoteTtsReadiness::Ready {
                    "Use MiniMax"
                } else {
                    "MiniMax Not Ready"
                },
                if readiness == tts::RemoteTtsReadiness::Ready {
                    theme.accent_orange
                } else {
                    theme.text_tertiary
                },
            )
            .id("settings-tts-minimax-activate")
            .debug_selector(|| "settings-tts-minimax-activate".to_string())
            .when(readiness == tts::RemoteTtsReadiness::Ready, |button| {
                button.on_click(cx.listener(MultisoulApp::on_voice_use_minimax_tts))
            }),
        )
}

fn minimax_model_selector(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let selected = state
        .values()
        .minimax_tts_model_id()
        .map(tts::TtsModelId::as_str);
    setting_row(
        theme,
        "Model",
        "Turbo is recommended for interactive replies; HD prioritizes quality",
    )
    .child(
        div()
            .flex()
            .gap(px(8.0))
            .child(
                action_button(
                    theme,
                    if selected == Some(tts::MINIMAX_SPEECH_TURBO_MODEL_ID) {
                        "Speech 2.8 Turbo · Selected"
                    } else {
                        "Speech 2.8 Turbo"
                    },
                    theme.accent_blue,
                )
                .id("settings-tts-minimax-model-turbo")
                .debug_selector(|| "settings-tts-minimax-model-turbo".to_string())
                .on_click(cx.listener(MultisoulApp::on_voice_select_minimax_turbo)),
            )
            .child(
                action_button(
                    theme,
                    if selected == Some(tts::MINIMAX_SPEECH_HD_MODEL_ID) {
                        "Speech 2.8 HD · Selected"
                    } else {
                        "Speech 2.8 HD"
                    },
                    theme.accent_blue,
                )
                .id("settings-tts-minimax-model-hd")
                .debug_selector(|| "settings-tts-minimax-model-hd".to_string())
                .on_click(cx.listener(MultisoulApp::on_voice_select_minimax_hd)),
            ),
    )
}

fn minimax_voice_catalog(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let mut catalog = div().w_full().flex().flex_col().gap(px(8.0)).child(
        div()
            .text_size(px(13.0))
            .text_color(rgba(theme.text_secondary))
            .child("Available Voices"),
    );
    for origin in [
        tts::TtsVoiceOrigin::System,
        tts::TtsVoiceOrigin::Cloned,
        tts::TtsVoiceOrigin::Generated,
    ] {
        let voices = state
            .filtered_minimax_voices()
            .into_iter()
            .filter(|voice| voice.origin() == origin)
            .collect::<Vec<_>>();
        if voices.is_empty() {
            continue;
        }
        let mut group = div()
            .w_full()
            .flex()
            .flex_wrap()
            .gap(px(6.0))
            .child(origin_label(origin));
        for (index, voice) in voices.into_iter().enumerate() {
            let voice_id = voice.id().clone();
            group = group.child(
                action_button(theme, voice.display_name().to_string(), theme.accent_blue)
                    .id((
                        "settings-tts-minimax-voice",
                        origin_index(origin) * 10_000 + index,
                    ))
                    .debug_selector(move || {
                        format!(
                            "settings-tts-minimax-voice-{}-{index}",
                            origin_index(origin)
                        )
                    })
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.on_voice_select_minimax_catalog_voice(voice_id.clone(), cx)
                    })),
            );
        }
        catalog = catalog.child(group);
    }
    catalog
}

fn readiness_label(readiness: tts::RemoteTtsReadiness) -> &'static str {
    match readiness {
        tts::RemoteTtsReadiness::Unconfigured => {
            "Add credentials, choose a model and enter a Voice ID."
        }
        tts::RemoteTtsReadiness::Configured => "Configured; connect to verify this account.",
        tts::RemoteTtsReadiness::Checking => "Checking MiniMax in the background…",
        tts::RemoteTtsReadiness::Ready => "Ready to activate. Preview is recommended.",
        tts::RemoteTtsReadiness::TemporarilyUnavailable => {
            "Temporarily unavailable; configuration was preserved."
        }
        tts::RemoteTtsReadiness::InvalidCredential => {
            "Credential rejected; correct the API Key and reconnect."
        }
        tts::RemoteTtsReadiness::VoiceUnavailable => {
            "The selected catalog Voice is no longer available."
        }
        tts::RemoteTtsReadiness::VoiceValidationRequired => {
            "Manual Voice ID requires Preview / Validate before activation."
        }
    }
}

fn origin_label(origin: tts::TtsVoiceOrigin) -> &'static str {
    match origin {
        tts::TtsVoiceOrigin::System => "System",
        tts::TtsVoiceOrigin::Cloned => "Cloned",
        tts::TtsVoiceOrigin::Generated => "Generated",
    }
}

fn origin_index(origin: tts::TtsVoiceOrigin) -> usize {
    match origin {
        tts::TtsVoiceOrigin::System => 0,
        tts::TtsVoiceOrigin::Cloned => 1,
        tts::TtsVoiceOrigin::Generated => 2,
    }
}
