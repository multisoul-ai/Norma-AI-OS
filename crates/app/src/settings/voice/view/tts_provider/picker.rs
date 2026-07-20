use super::super::{action_button, setting_row, with_alpha, VoiceSettingsInputs};
use crate::app::MultisoulApp;
use crate::settings::voice::VoiceSettingsState;
use gpui::{div, prelude::*, px, rgba, Context, Div, FontWeight, Stateful};
use ui::Theme;

mod provider_list;
use provider_list::provider_list;

pub(super) fn tts_provider_settings(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    if state.tts_picker_open() {
        div().w_full().child(picker(theme, state, inputs, cx))
    } else {
        div().w_full().child(current_voice(theme, state, cx))
    }
}

fn current_voice(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let active = state
        .values()
        .active_tts_provider_id()
        .map(tts::TtsProviderId::as_str);
    let configured = match active {
        Some(tts::LOCAL_TTS_PROVIDER_ID) => {
            state.values().tts_model_id().is_some() && state.values().tts_voice_id().is_some()
        }
        Some(tts::MINIMAX_TTS_PROVIDER_ID) => {
            state.values().minimax_tts_model_id().is_some()
                && state.values().minimax_tts_voice_id().is_some()
        }
        _ => false,
    };
    let (provider, voice, detail) = active_voice_summary(state);
    let mut preview = action_button(
        theme,
        "Preview",
        if configured {
            theme.accent_blue
        } else {
            theme.text_tertiary
        },
    )
    .id("settings-tts-preview-current")
    .debug_selector(|| "settings-tts-preview-current".to_string());
    if active == Some(tts::LOCAL_TTS_PROVIDER_ID) {
        if let (Some(model_id), Some(voice_id)) = (
            state.values().tts_model_id().cloned(),
            state.values().tts_voice_id().cloned(),
        ) {
            preview = preview.on_click(cx.listener(move |app, _, _, cx| {
                app.preview_tts_voice(model_id.clone(), voice_id.clone(), cx)
            }));
        }
    } else if active == Some(tts::MINIMAX_TTS_PROVIDER_ID) {
        preview = preview.on_click(cx.listener(MultisoulApp::on_voice_preview_minimax_tts));
    }
    div()
        .id("settings-tts-current-voice")
        .debug_selector(|| "settings-tts-current-voice".to_string())
        .w_full()
        .flex()
        .flex_col()
        .gap(px(14.0))
        .p(px(16.0))
        .rounded(px(14.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.text_primary, 0x24)))
        .bg(rgba(with_alpha(theme.panel_high, 0xa8)))
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgba(theme.text_primary))
                        .child("Current Voice"),
                )
                .child(status_badge(
                    if configured {
                        "ACTIVE"
                    } else {
                        "NOT CONFIGURED"
                    },
                    if configured {
                        theme.accent_green
                    } else {
                        theme.text_tertiary
                    },
                )),
        )
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(16.0))
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgba(theme.accent_orange))
                                .child(provider),
                        )
                        .child(
                            div()
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgba(theme.text_primary))
                                .child(voice),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgba(theme.text_secondary))
                                .child(detail),
                        ),
                )
                .child(preview),
        )
        .child(
            div().w_full().flex().justify_end().child(
                action_button(theme, "Change Voice…", theme.accent_blue)
                    .id("settings-tts-change-voice")
                    .debug_selector(|| "settings-tts-change-voice".to_string())
                    .on_click(cx.listener(MultisoulApp::on_voice_open_tts_picker)),
            ),
        )
}

fn picker(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let provider = state
        .tts_picker_provider_id()
        .unwrap_or(tts::LOCAL_TTS_PROVIDER_ID);
    div()
        .id("settings-tts-picker")
        .debug_selector(|| "settings-tts-picker".to_string())
        .w_full()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .p(px(14.0))
        .rounded(px(14.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.text_primary, 0x2e)))
        .bg(rgba(with_alpha(theme.panel_high, 0xb8)))
        .child(
            div()
                .text_size(px(15.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(theme.text_primary))
                .child("Choose a Voice"),
        )
        .child(
            div()
                .id("settings-tts-picker-current-notice")
                .debug_selector(|| "settings-tts-picker-current-notice".to_string())
                .w_full()
                .px(px(10.0))
                .py(px(8.0))
                .rounded(px(8.0))
                .bg(rgba(with_alpha(theme.accent_blue, 0x12)))
                .text_size(px(12.0))
                .text_color(rgba(theme.text_secondary))
                .child("Your current voice stays active until you choose Use This Voice."),
        )
        .child(
            div()
                .w_full()
                .flex()
                .gap(px(12.0))
                .child(provider_list(theme, state, provider, cx))
                .child(div().min_w(px(0.0)).flex_1().child(
                    if provider == tts::MINIMAX_TTS_PROVIDER_ID {
                        super::minimax_configuration(theme, state, inputs, cx)
                    } else {
                        local_voice_picker(theme, state, cx)
                    },
                )),
        )
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_end()
                .gap(px(8.0))
                .child(
                    action_button(theme, "Cancel", theme.text_secondary)
                        .id("settings-tts-picker-cancel")
                        .debug_selector(|| "settings-tts-picker-cancel".to_string())
                        .on_click(cx.listener(MultisoulApp::on_voice_cancel_tts_picker)),
                )
                .child({
                    let ready = state.tts_picker_ready();
                    action_button(
                        theme,
                        if ready {
                            "Use This Voice"
                        } else {
                            "Choose a Ready Voice"
                        },
                        if ready {
                            theme.accent_blue
                        } else {
                            theme.text_tertiary
                        },
                    )
                    .id("settings-tts-picker-apply")
                    .debug_selector(|| "settings-tts-picker-apply".to_string())
                    .when(ready, |button| {
                        button.on_click(cx.listener(MultisoulApp::on_voice_apply_tts_picker))
                    })
                }),
        )
}

fn local_voice_picker(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let selected_model = state.tts_picker_local_model_id();
    let selected_voice = state.tts_picker_local_voice_id();
    let mut voices = div().w_full().flex().flex_col().gap(px(6.0));
    let mut ready_voice_count = 0_usize;
    for (model_index, entry) in state.tts_model_manager().entries().iter().enumerate() {
        if !matches!(
            entry.installation(),
            crate::settings::voice::TtsModelInstallState::Ready
        ) {
            continue;
        }
        for (voice_index, voice) in entry.descriptor().voices().iter().enumerate() {
            ready_voice_count += 1;
            let model_id = entry.descriptor().id().clone();
            let voice_id = voice.id().clone();
            let preview_model = model_id.clone();
            let preview_voice = voice_id.clone();
            let selected = selected_model == Some(&model_id) && selected_voice == Some(&voice_id);
            let selector_index = model_index * 100 + voice_index;
            voices = voices.child(
                div()
                    .id(("settings-tts-picker-local-voice", selector_index))
                    .debug_selector(move || {
                        format!("settings-tts-picker-local-voice-{model_index}-{voice_index}")
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
                        if selected {
                            theme.accent_blue
                        } else {
                            theme.edge_highlight
                        },
                        if selected { 0x72 } else { 0x32 },
                    )))
                    .cursor_pointer()
                    .child(
                        action_button(theme, "▶", theme.accent_blue)
                            .id(("settings-tts-picker-local-preview", selector_index))
                            .debug_selector(move || {
                                format!(
                                    "settings-tts-picker-local-preview-{model_index}-{voice_index}"
                                )
                            })
                            .on_click(cx.listener(move |app, _, _, cx| {
                                cx.stop_propagation();
                                app.preview_tts_voice(
                                    preview_model.clone(),
                                    preview_voice.clone(),
                                    cx,
                                )
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
                                    .child(voice.display_name().to_string()),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(rgba(theme.text_tertiary))
                                    .child(entry.descriptor().display_name().to_string()),
                            ),
                    )
                    .when(selected, |row| {
                        row.child(status_badge("DRAFT", theme.accent_blue))
                    })
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.on_voice_pick_local_tts_voice(model_id.clone(), voice_id.clone(), cx)
                    })),
            );
        }
    }
    if ready_voice_count == 0 {
        voices = voices.child(
            div()
                .w_full()
                .p(px(12.0))
                .rounded(px(8.0))
                .bg(rgba(with_alpha(theme.text_primary, 0x08)))
                .text_size(px(12.0))
                .text_color(rgba(theme.text_secondary))
                .child("No On-device voice is ready. Download a model to continue."),
        );
    }
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(setting_row(
            theme,
            "On-device",
            "Synthesized locally and available offline",
        ))
        .child(voices)
        .child(
            action_button(theme, "Manage Models…", theme.accent_blue)
                .id("settings-tts-picker-manage-models")
                .debug_selector(|| "settings-tts-picker-manage-models".to_string())
                .on_click(cx.listener(MultisoulApp::on_voice_expand_tts_models)),
        )
}

pub(super) fn status_badge(label: &'static str, color: u32) -> Div {
    div()
        .px(px(7.0))
        .py(px(3.0))
        .rounded(px(6.0))
        .bg(rgba(with_alpha(color, 0x20)))
        .text_size(px(9.0))
        .font_weight(FontWeight::BOLD)
        .text_color(rgba(color))
        .child(label)
}

fn active_voice_summary(state: &VoiceSettingsState) -> (&'static str, String, String) {
    match state
        .values()
        .active_tts_provider_id()
        .map(tts::TtsProviderId::as_str)
    {
        Some(tts::LOCAL_TTS_PROVIDER_ID) => {
            let catalog = tts::builtin_model_catalog();
            let model = state
                .values()
                .tts_model_id()
                .and_then(|id| catalog.find(id));
            let voice = model
                .and_then(|model| state.values().tts_voice_id().and_then(|id| model.voice(id)))
                .map(tts::TtsVoiceDescriptor::display_name)
                .unwrap_or("No voice selected")
                .to_string();
            let detail = model
                .map(|model| format!("{} · Available offline", model.display_name()))
                .unwrap_or_else(|| "Install an On-device model to enable speech".to_string());
            ("On-device", voice, detail)
        }
        Some(tts::MINIMAX_TTS_PROVIDER_ID) => {
            let voice = state
                .values()
                .minimax_tts_voice_id()
                .map(tts::TtsVoiceId::as_str)
                .unwrap_or("No voice selected")
                .to_string();
            let model = state
                .values()
                .minimax_tts_model_id()
                .map(tts::TtsModelId::as_str)
                .unwrap_or("No model selected");
            ("MiniMax", voice, format!("{model} · Cloud voice"))
        }
        _ => (
            "Not configured",
            "No voice selected".to_string(),
            "Choose an On-device or cloud voice".to_string(),
        ),
    }
}
