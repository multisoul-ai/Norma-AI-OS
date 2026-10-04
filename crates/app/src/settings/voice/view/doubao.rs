use super::components::VoiceAction;
use super::{settings_group, with_alpha, VoiceSettingsInputs};
use crate::app::MultisoulApp;
use crate::settings::voice::{
    DesktopVoiceMode, VoiceRequirement, VoiceRequirementState, VoiceSettingsField,
    VoiceSettingsState,
};
use gpui::{div, prelude::*, px, rgba, Context, Div, FontWeight, Stateful};
use ui::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use ui::Theme;

pub(super) fn desktop_mode_picker(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let mode = state.values().desktop_mode();
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(
            div()
                .text_size(px(14.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgba(theme.text_primary))
                .child("Conversation mode"),
        )
        .child(
            div()
                .w_full()
                .flex()
                .flex_wrap()
                .gap(px(12.0))
                .child(mode_card(
                    theme,
                    DesktopVoiceMode::Legacy,
                    mode == DesktopVoiceMode::Legacy,
                    cx,
                ))
                .child(mode_card(
                    theme,
                    DesktopVoiceMode::Doubao,
                    mode == DesktopVoiceMode::Doubao,
                    cx,
                )),
        )
}

fn mode_card(
    theme: Theme,
    mode: DesktopVoiceMode,
    selected: bool,
    cx: &Context<MultisoulApp>,
) -> Stateful<Div> {
    let (selector, title, hint) = match mode {
        DesktopVoiceMode::Legacy => (
            "settings-voice-desktop-mode-legacy",
            "Voice Commands",
            "Give a voice command or listen continuously.",
        ),
        DesktopVoiceMode::Doubao => (
            "settings-voice-desktop-mode-doubao",
            "Doubao 2.0 Voice",
            "Have a real-time conversation with Norma.",
        ),
    };
    let variant = ButtonCustomVariant::new(cx)
        .color(
            rgba(if selected {
                with_alpha(theme.accent_blue, 0x16)
            } else {
                theme.panel_high
            })
            .into(),
        )
        .foreground(rgba(theme.text_primary).into())
        .border(
            rgba(with_alpha(
                if selected {
                    theme.accent_blue
                } else {
                    theme.text_secondary
                },
                if selected { 0x80 } else { 0x25 },
            ))
            .into(),
        )
        .hover(rgba(with_alpha(theme.accent_blue, 0x25)).into())
        .active(rgba(with_alpha(theme.accent_blue, 0x38)).into());
    div()
        .id(selector)
        .debug_selector(move || selector.to_string())
        .flex_1()
        .min_w(px(230.0))
        .child(
            Button::new(selector)
                .custom(variant)
                .w_full()
                .h(px(88.0))
                .justify_start()
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title)
                                .child(
                                    div()
                                        .text_color(rgba(theme.accent_blue))
                                        .child(if selected { "✓" } else { "" }),
                                ),
                        )
                        .child(
                            div()
                                .whitespace_normal()
                                .text_size(px(12.0))
                                .text_color(rgba(theme.text_secondary))
                                .child(hint),
                        ),
                )
                .on_activate(cx.listener(move |app, _, _, cx| {
                    if mode == DesktopVoiceMode::Legacy {
                        app.stop_doubao_voice(cx);
                    } else {
                        app.stop_continuous_listening(cx);
                        app.voice.stop_for_model_switch();
                        app.cancel_agent_announcements();
                    }
                    app.voice_settings.select_desktop_mode(mode);
                    app.persist_voice_settings_in_background(cx);
                    cx.notify();
                })),
        )
}

pub(super) fn doubao_settings(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    voice_shortcut_label: &str,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let configured = state.values().doubao_credential_ref().is_some();
    let microphone = state.requirement_state(VoiceRequirement::Microphone);
    let microphone_ready = microphone == VoiceRequirementState::Ready;
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
                            "API Key saved in this Mac's local database"
                        } else {
                            "Enter your Volcengine Ark API Key for ASR 2.0, Chat and TTS 2.0"
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
                .when(
                    matches!(microphone, VoiceRequirementState::Missing | VoiceRequirementState::Failed),
                    |group| {
                        group.child(
                            super::action_button(
                                theme,
                                if microphone == VoiceRequirementState::Failed {
                                    "Open Microphone Settings"
                                } else {
                                    "Grant Access"
                                },
                                theme.accent_blue,
                            )
                            .id("settings-voice-doubao-microphone-action")
                            .debug_selector(|| "settings-voice-doubao-microphone-action".to_string())
                            .on_click(cx.listener(MultisoulApp::on_voice_setup_request_microphone)),
                        )
                    },
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgba(theme.text_secondary))
                        .child(format!("Uses macOS system input and output. Press {voice_shortcut_label} to start or end the call.")),
                ),
        )
}
