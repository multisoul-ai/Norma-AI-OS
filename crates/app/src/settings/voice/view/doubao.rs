use super::{settings_group, VoiceSettingsInputs};
use crate::app::MultisoulApp;
use crate::settings::voice::{
    DesktopVoiceMode, VoiceRequirement, VoiceRequirementState, VoiceSettingsField,
    VoiceSettingsState,
};
use gpui::{div, prelude::*, px, rgba, Context, Div, FontWeight};
use ui::Theme;

pub(super) fn desktop_mode_picker(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let mode = state.values().desktop_mode();
    settings_group(theme, "Voice Mode").child(
        div()
            .flex()
            .gap(px(8.0))
            .child(
                super::interaction::mode_option(
                    theme,
                    "Voice Commands",
                    mode == DesktopVoiceMode::Legacy,
                )
                .id("settings-voice-desktop-mode-legacy")
                .debug_selector(|| "settings-voice-desktop-mode-legacy".to_string())
                .on_click(cx.listener(|app, _, _, cx| {
                    app.stop_doubao_voice(cx);
                    app.voice_settings
                        .select_desktop_mode(DesktopVoiceMode::Legacy);
                    app.persist_voice_settings_in_background(cx);
                    cx.notify();
                })),
            )
            .child(
                super::interaction::mode_option(
                    theme,
                    "Doubao Duplex",
                    mode == DesktopVoiceMode::Doubao,
                )
                .id("settings-voice-desktop-mode-doubao")
                .debug_selector(|| "settings-voice-desktop-mode-doubao".to_string())
                .on_click(cx.listener(|app, _, _, cx| {
                    app.stop_continuous_listening(cx);
                    app.voice.stop_for_model_switch();
                    app.cancel_agent_announcements();
                    app.voice_settings
                        .select_desktop_mode(DesktopVoiceMode::Doubao);
                    app.persist_voice_settings_in_background(cx);
                    cx.notify();
                })),
            ),
    )
}

pub(super) fn doubao_settings(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let configured = state.values().doubao_credential_ref().is_some();
    let microphone_ready =
        state.requirement_state(VoiceRequirement::Microphone) == VoiceRequirementState::Ready;
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(20.0))
        .child(
            settings_group(theme, "Doubao Connection")
                .child(
                    div()
                        .id("settings-voice-doubao-readiness")
                        .debug_selector(|| "settings-voice-doubao-readiness".to_string())
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgba(if configured && microphone_ready {
                            theme.accent_green
                        } else {
                            theme.accent_orange
                        }))
                        .child(if !microphone_ready {
                            "Microphone permission required"
                        } else if !configured {
                            "API Key required"
                        } else {
                            "Ready to connect"
                        }),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgba(theme.text_secondary))
                        .child(if configured {
                            "API Key saved in macOS Keychain"
                        } else {
                            "Enter a Doubao Voice API Key with Seeduplex 3.0 access"
                        }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .child(inputs.input(VoiceSettingsField::DoubaoApiKey))
                        .when(configured, |row| {
                            row.child(
                                super::action_button(theme, "Remove", theme.destructive)
                                    .id("settings-doubao-api-key-clear")
                                    .debug_selector(|| "settings-doubao-api-key-clear".to_string())
                                    .on_click(cx.listener(|app, _, _, cx| {
                                        app.stop_doubao_voice(cx);
                                        app.voice_settings.clear_doubao_credential();
                                        app.persist_voice_settings_in_background(cx);
                                        cx.notify();
                                    })),
                            )
                        }),
                ),
        )
        .child(
            settings_group(theme, "Audio")
                .child(
                    div()
                        .id("settings-voice-doubao-system-input")
                        .debug_selector(|| "settings-voice-doubao-system-input".to_string())
                        .text_size(px(13.0))
                        .text_color(rgba(theme.text_primary))
                        .child("Microphone · macOS system default"),
                )
                .child(
                    div()
                        .id("settings-voice-doubao-microphone-status")
                        .debug_selector(|| "settings-voice-doubao-microphone-status".to_string())
                        .text_size(px(11.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgba(if microphone_ready {
                            theme.accent_green
                        } else {
                            theme.accent_orange
                        }))
                        .child(if microphone_ready {
                            "●  Microphone permission authorized"
                        } else {
                            "●  Microphone permission required"
                        }),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgba(theme.text_secondary))
                        .child("Uses the macOS system input and output devices. Press ⌘E to start or end the call."),
                ),
        )
}
