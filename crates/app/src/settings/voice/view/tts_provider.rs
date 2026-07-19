use super::{action_button, setting_row, text_setting_row, with_alpha, VoiceSettingsInputs};
use crate::app::MultisoulApp;
use crate::settings::voice::{VoiceSettingsField, VoiceSettingsState};
use gpui::{div, prelude::*, px, rgba, Context, Div, FontWeight};
use ui::Theme;

mod controller;
mod picker;

pub(super) fn tts_provider_settings(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    picker::tts_provider_settings(theme, state, inputs, cx)
}

pub(super) fn minimax_configuration(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let readiness = state.tts_picker_minimax_readiness();
    let connected = state.minimax_account_status() == tts::RemoteAccountStatus::Ready;
    let connection_expanded = state.tts_picker_connection_expanded();
    let manual = state.tts_picker_minimax_provenance() == tts::VoiceSelectionProvenance::Manual;
    let mut content = div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(9.0))
                .rounded(px(9.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.edge_highlight, 0x42)))
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgba(theme.text_primary))
                                .child(if connected {
                                    "MiniMax · Connected"
                                } else {
                                    "Connect MiniMax"
                                }),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgba(if connected {
                                    theme.accent_green
                                } else {
                                    theme.text_tertiary
                                }))
                                .child(if connected {
                                    "Account ready · Key stored in Keychain"
                                } else {
                                    "Connect before choosing a cloud Voice"
                                }),
                        ),
                )
                .child(
                    action_button(
                        theme,
                        if connection_expanded {
                            "Done"
                        } else {
                            "Manage…"
                        },
                        theme.accent_blue,
                    )
                    .id("settings-tts-minimax-manage-connection")
                    .debug_selector(|| "settings-tts-minimax-manage-connection".to_string())
                    .on_click(cx.listener(MultisoulApp::on_voice_toggle_minimax_connection)),
                ),
        )
        .when(connection_expanded, |content| {
            content
                .child(text_setting_row(
                    theme,
                    inputs,
                    VoiceSettingsField::MiniMaxApiKey,
                    "MiniMax API Key",
                    "Validated credentials are stored only in macOS Keychain",
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            action_button(
                                theme,
                                if state.minimax_account_status()
                                    == tts::RemoteAccountStatus::Checking
                                {
                                    "Connecting…"
                                } else {
                                    "Connect"
                                },
                                theme.accent_blue,
                            )
                            .id("settings-tts-minimax-connect")
                            .debug_selector(|| "settings-tts-minimax-connect".to_string())
                            .on_click(cx.listener(MultisoulApp::on_voice_connect_minimax_tts)),
                        )
                        .when(state.minimax_credential_configured(), |actions| {
                            actions.child(
                                action_button(theme, "Clear", theme.destructive)
                                    .id("settings-tts-minimax-clear-credential")
                                    .debug_selector(|| {
                                        "settings-tts-minimax-clear-credential".to_string()
                                    })
                                    .on_click(cx.listener(
                                        MultisoulApp::on_voice_clear_minimax_tts_credential,
                                    )),
                            )
                        }),
                )
        });
    if state.minimax_credential_configured() {
        content = content
            .child(minimax_model_selector(theme, state, cx))
            .when(
                !state.minimax_voice_catalog().voices().is_empty(),
                |content| {
                    content
                        .child(text_setting_row(
                            theme,
                            inputs,
                            VoiceSettingsField::MiniMaxVoiceSearch,
                            "Search Voices",
                            "Search this account by name or Voice ID",
                            cx,
                        ))
                        .child(minimax_voice_catalog(theme, state, cx))
                },
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .p(px(10.0))
                    .rounded(px(9.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.edge_highlight, 0x42)))
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgba(theme.text_primary))
                            .child("Use a Voice ID"),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgba(theme.text_tertiary))
                            .child("Enter a known Voice ID that is not listed above."),
                    )
                    .child(inputs.input(VoiceSettingsField::MiniMaxVoiceId)),
            )
            .child(
                div()
                    .text_size(px(11.0))
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
                action_button(
                    theme,
                    if state.minimax_preview_in_progress() {
                        "Previewing…"
                    } else if manual {
                        "Verify & Preview"
                    } else {
                        "Preview"
                    },
                    theme.accent_blue,
                )
                .id("settings-tts-minimax-preview")
                .debug_selector(|| "settings-tts-minimax-preview".to_string())
                .on_click(cx.listener(MultisoulApp::on_voice_preview_minimax_tts)),
            );
    }
    content
}

fn minimax_model_selector(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let selected = state
        .tts_picker_minimax_model_id()
        .or_else(|| state.values().minimax_tts_model_id())
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
    let selected = state.tts_picker_minimax_voice_id();
    let mut catalog = div().w_full().flex().flex_col().gap(px(8.0));
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
        let mut group = div().w_full().flex().flex_col().gap(px(6.0)).child(
            div()
                .text_size(px(10.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(theme.text_tertiary))
                .child(origin_label(origin)),
        );
        for (index, voice) in voices.into_iter().enumerate() {
            let voice_id = voice.id().clone();
            let preview_voice_id = voice_id.clone();
            let selected_here = selected == Some(&voice_id);
            let display_name = voice.display_name().to_string();
            let voice_id_label = voice_id.as_str().to_string();
            group = group.child(
                div()
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
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(10.0))
                    .py(px(8.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(rgba(with_alpha(
                        if selected_here {
                            theme.accent_blue
                        } else {
                            theme.edge_highlight
                        },
                        if selected_here { 0x72 } else { 0x32 },
                    )))
                    .cursor_pointer()
                    .child(
                        action_button(theme, "▶", theme.accent_blue)
                            .id((
                                "settings-tts-minimax-preview-voice",
                                origin_index(origin) * 10_000 + index,
                            ))
                            .debug_selector(move || {
                                format!(
                                    "settings-tts-minimax-preview-voice-{}-{index}",
                                    origin_index(origin)
                                )
                            })
                            .on_click(cx.listener(move |app, _, _, cx| {
                                cx.stop_propagation();
                                app.preview_minimax_catalog_voice(preview_voice_id.clone(), cx)
                            })),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgba(theme.text_primary))
                                    .child(display_name),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(rgba(theme.text_tertiary))
                                    .child(voice_id_label),
                            ),
                    )
                    .when(selected_here, |row| {
                        row.child(
                            div()
                                .text_size(px(10.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgba(theme.accent_blue))
                                .child("DRAFT"),
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
            "Manual Voice ID requires Verify & Preview before it can be used."
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
