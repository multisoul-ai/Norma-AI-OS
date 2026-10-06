use super::{action_button, setting_row, text_setting_row, with_alpha, VoiceSettingsInputs};
use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use crate::settings::voice::{VoiceSettingsField, VoiceSettingsState};
use gpui::{div, prelude::*, px, rgba, Context, Div};
use ui::Theme;

mod controller;
mod picker;
mod voice_list;

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
    let mut content =
        div()
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
                            .child(div().settings_text(TextRole::RowTitle, theme).child(
                                if connected {
                                    ui::i18n::text("voice.minimax_connected")
                                } else {
                                    ui::i18n::text("voice.connect_minimax")
                                },
                            ))
                            .child(
                                div()
                                    .settings_font(TextRole::Caption)
                                    .text_color(rgba(if connected {
                                        theme.accent_green
                                    } else {
                                        theme.text_tertiary
                                    }))
                                    .child(if connected {
                                        ui::i18n::text("voice.account_ready_key_stored_locally")
                                    } else {
                                        ui::i18n::text(
                                            "voice.connect_before_choosing_a_cloud_voice",
                                        )
                                    }),
                            ),
                    )
                    .child(
                        action_button(
                            theme,
                            if connection_expanded {
                                "Done"
                            } else {
                                ui::i18n::text("voice.manage")
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
                        ui::i18n::text("voice.minimax_api_key"),
                        ui::i18n::text(
                            "voice.validated_credentials_are_stored_in_this_mac_s_local_database",
                        ),
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
                                        ui::i18n::text("voice.connecting")
                                    } else {
                                        ui::i18n::text("voice.connect")
                                    },
                                    theme.accent_blue,
                                )
                                .id("settings-tts-minimax-connect")
                                .debug_selector(|| "settings-tts-minimax-connect".to_string())
                                .on_click(cx.listener(MultisoulApp::on_voice_connect_minimax_tts)),
                            )
                            .when(state.minimax_credential_configured(), |actions| {
                                actions.child(
                                    action_button(
                                        theme,
                                        ui::i18n::text("voice.clear"),
                                        theme.destructive,
                                    )
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
                            ui::i18n::text("voice.search_voices"),
                            ui::i18n::text("voice.search_this_account_by_name_or_voice_id"),
                            cx,
                        ))
                        .child(voice_list::minimax_voice_catalog(theme, state, cx))
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
                            .settings_text(TextRole::RowTitle, theme)
                            .child(ui::i18n::text("voice.use_a_voice_id")),
                    )
                    .child(
                        div()
                            .settings_text(TextRole::Helper, theme)
                            .child(ui::i18n::text(
                                "voice.enter_a_known_voice_id_that_is_not_listed_above",
                            )),
                    )
                    .child(inputs.input(VoiceSettingsField::MiniMaxVoiceId)),
            )
            .child(
                div()
                    .settings_font(TextRole::Label)
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
                        ui::i18n::text("voice.previewing")
                    } else if manual {
                        ui::i18n::text("voice.verify_preview")
                    } else {
                        ui::i18n::text("voice.preview")
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
        ui::i18n::text("voice.model"),
        ui::i18n::text("voice.turbo_is_recommended_for_interactive_replies_hd_prioritizes_quality"),
    )
    .child(
        div()
            .flex()
            .gap(px(8.0))
            .child(
                action_button(
                    theme,
                    if selected == Some(tts::MINIMAX_SPEECH_TURBO_MODEL_ID) {
                        ui::i18n::text("voice.speech_2_8_turbo_selected")
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
                        ui::i18n::text("voice.speech_2_8_hd_selected")
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

fn readiness_label(readiness: tts::RemoteTtsReadiness) -> &'static str {
    match readiness {
        tts::RemoteTtsReadiness::Unconfigured => {
            ui::i18n::text("voice.add_credentials_choose_a_model_and_enter_a_voice_id")
        }
        tts::RemoteTtsReadiness::Configured => {
            ui::i18n::text("voice.configured_connect_to_verify_this_account")
        }
        tts::RemoteTtsReadiness::Checking => {
            ui::i18n::text("voice.checking_minimax_in_the_background")
        }
        tts::RemoteTtsReadiness::Ready => {
            ui::i18n::text("voice.ready_to_activate_preview_is_recommended")
        }
        tts::RemoteTtsReadiness::TemporarilyUnavailable => {
            ui::i18n::text("voice.temporarily_unavailable_configuration_was_preserved")
        }
        tts::RemoteTtsReadiness::InvalidCredential => {
            ui::i18n::text("voice.credential_rejected_correct_the_api_key_and_reconnect")
        }
        tts::RemoteTtsReadiness::VoiceUnavailable => {
            ui::i18n::text("voice.the_selected_catalog_voice_is_no_longer_available")
        }
        tts::RemoteTtsReadiness::VoiceValidationRequired => {
            ui::i18n::text("voice.manual_voice_id_requires_verify_preview_before_it_can_be_used")
        }
    }
}
