use super::components::VoiceAction;
use super::{settings_group, with_alpha, VoiceSettingsInputs};
use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use crate::settings::voice::{
    DesktopVoiceMode, VoiceRequirement, VoiceRequirementState, VoiceSettingsField,
    VoiceSettingsState,
};
use gpui::{div, prelude::*, px, rgba, Context, Div, Stateful, Window};
use ui::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use ui::Theme;
mod catalog;
mod item;
mod menu_state;
mod voices;

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
                .settings_text(TextRole::Headline, theme)
                .child(ui::i18n::text("voice.conversation_mode")),
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
            ui::i18n::text("voice.voice_commands"),
            ui::i18n::text("voice.give_a_voice_command_or_listen_continuously"),
        ),
        DesktopVoiceMode::Doubao => (
            "settings-voice-desktop-mode-doubao",
            ui::i18n::text("voice.doubao_2_0_voice"),
            ui::i18n::text("voice.have_a_real_time_conversation_with_norma"),
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
                                .settings_font(TextRole::RowTitle)
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
                                .settings_text(TextRole::Helper, theme)
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
    window: &mut Window,
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
            settings_group(theme, ui::i18n::text("voice.doubao_connection"))
                .child(
                    div()
                        .id("settings-voice-doubao-readiness")
                        .debug_selector(|| "settings-voice-doubao-readiness".to_string())
                        .settings_font(TextRole::Label)
                        .text_color(rgba(if configured && microphone_ready {
                            theme.accent_green
                        } else {
                            theme.accent_orange
                        }))
                        .child(if !microphone_ready {
                            ui::i18n::text("voice.microphone_permission_required")
                        } else if !configured {
                            ui::i18n::text("voice.api_key_required")
                        } else {
                            ui::i18n::text("voice.ready_to_connect")
                        }),
                )
                .child(
                    div()
                        .settings_text(TextRole::Helper, theme)
                        .child(if configured {
                            ui::i18n::text("voice.api_key_saved_in_this_mac_s_local_database")
                        } else {
                            ui::i18n::text(
                                "voice.enter_your_volcengine_ark_api_key_for_asr_2_0_chat",
                            )
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
                                super::action_button(
                                    theme,
                                    ui::i18n::text("common.remove"),
                                    theme.destructive,
                                )
                                .id("settings-doubao-api-key-clear")
                                .debug_selector(|| "settings-doubao-api-key-clear".to_string())
                                .on_click(cx.listener(
                                    |app, _, _, cx| {
                                        app.stop_doubao_voice(cx);
                                        app.voice_settings.clear_doubao_credential();
                                        app.persist_voice_settings_in_background(cx);
                                        cx.notify();
                                    },
                                )),
                            )
                        }),
                ),
        )
        .child(
            settings_group(theme, ui::i18n::text("voice.audio"))
                .child(
                    div()
                        .id("settings-voice-doubao-system-input")
                        .debug_selector(|| "settings-voice-doubao-system-input".to_string())
                        .settings_text(TextRole::Body, theme)
                        .child(ui::i18n::text("voice.microphone_macos_system_default")),
                )
                .child(
                    div()
                        .id("settings-voice-doubao-microphone-status")
                        .debug_selector(|| "settings-voice-doubao-microphone-status".to_string())
                        .settings_font(TextRole::Label)
                        .text_color(rgba(if microphone_ready {
                            theme.accent_green
                        } else {
                            theme.accent_orange
                        }))
                        .child(if microphone_ready {
                            ui::i18n::text("voice.microphone_permission_authorized")
                        } else {
                            ui::i18n::text("voice.microphone_permission_required_2")
                        }),
                )
                .when(
                    matches!(
                        microphone,
                        VoiceRequirementState::Missing | VoiceRequirementState::Failed
                    ),
                    |group| {
                        group.child(
                            super::action_button(
                                theme,
                                if microphone == VoiceRequirementState::Failed {
                                    ui::i18n::text("chrome.open_microphone_settings")
                                } else {
                                    ui::i18n::text("voice.grant_access")
                                },
                                theme.accent_blue,
                            )
                            .id("settings-voice-doubao-microphone-action")
                            .debug_selector(|| {
                                "settings-voice-doubao-microphone-action".to_string()
                            })
                            .on_click(cx.listener(MultisoulApp::on_voice_setup_request_microphone)),
                        )
                    },
                )
                .child(
                    div()
                        .settings_text(TextRole::Helper, theme)
                        .child(ui::i18n::format(
                        "voice.uses_macos_system_input_and_output_press_voice_shortcut_label_to",
                        &[("voice_shortcut_label", (voice_shortcut_label).to_string())],
                    )),
                ),
        )
        .child(voices::voice_picker(theme, state, window, cx))
}
