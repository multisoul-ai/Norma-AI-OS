use super::{action_button, with_alpha};
use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use crate::settings::voice::{
    DesktopVoiceMode, VoiceRequirement, VoiceRequirementState, VoiceSettingsState,
};
use gpui::{div, prelude::*, px, rgba, Context, Div};
use ui::Theme;

pub(super) fn input_device_row(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let is_live = state.values().desktop_mode() == DesktopVoiceMode::GptLive;
    let microphone_ready =
        state.requirement_state(VoiceRequirement::Microphone) == VoiceRequirementState::Ready;
    let selected_id = state.values().input_device_id();
    let selected_label = selected_id
        .and_then(|selected| {
            state
                .input_devices()
                .iter()
                .find(|device| device.id == selected)
        })
        .map(|device| device.name.clone())
        .unwrap_or_else(|| ui::i18n::text("voice.system_default").to_string());
    let mut options = div().flex().flex_col().gap(px(8.0)).child(
        input_device_option(
            theme,
            ui::i18n::text("voice.system_default").to_string(),
            selected_id.is_none(),
        )
        .id("settings-voice-input-device-default")
        .debug_selector(|| "settings-voice-input-device-default".to_string())
        .on_click(cx.listener(MultisoulApp::on_voice_settings_use_default_input_device)),
    );

    for (index, device) in state.input_devices().iter().enumerate() {
        let id = device.id.clone();
        let selected = selected_id == Some(device.id.as_str());
        let label = format!(
            "{}{}",
            device.name,
            if device.is_default {
                ui::i18n::text("voice.system_default_2")
            } else {
                ""
            }
        );
        options = options.child(
            input_device_option(theme, label, selected)
                .id(("settings-voice-input-device", index))
                .debug_selector(move || format!("settings-voice-input-device-{index}"))
                .on_click(cx.listener(move |app, _, _window, cx| {
                    app.stop_doubao_voice(cx);
                    app.stop_gpt_live_voice(cx);
                    app.voice_settings.select_input_device(Some(id.clone()));
                    app.voice_settings.close_input_device_picker();
                    app.persist_voice_settings_in_background(cx);
                    cx.notify();
                })),
        );
    }

    let picker = div()
        .id("settings-voice-input-device-picker")
        .debug_selector(|| "settings-voice-input-device-picker".to_string())
        .flex_none()
        .min_w(px(190.0))
        .max_w(px(300.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .px(px(12.0))
        .py(px(9.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.text_primary, 0x2e)))
        .bg(rgba(with_alpha(theme.panel_high, 0xb8)))
        .settings_text(TextRole::Body, theme)
        .cursor_pointer()
        .on_click(cx.listener(MultisoulApp::on_voice_settings_toggle_input_device_picker))
        .child(selected_label)
        .child("⌄");

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
                .gap(px(16.0))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .child(
                            div()
                                .settings_text(TextRole::RowTitle, theme)
                                .child(ui::i18n::text("voice.microphone")),
                        )
                        .child(
                            div()
                                .settings_text(TextRole::Helper, theme)
                                .child(ui::i18n::text(if is_live {
                                    "voice.live_input_device_hint"
                                } else {
                                    "voice.input_used_for_speech_recognition"
                                })),
                        ),
                )
                .child(picker),
        )
        .child(
            div()
                .id("settings-voice-legacy-recognition-status")
                .debug_selector(|| "settings-voice-legacy-recognition-status".to_string())
                .settings_font(TextRole::Label)
                .text_color(rgba(if is_live && !microphone_ready {
                    theme.accent_orange
                } else {
                    theme.accent_green
                }))
                .child(ui::i18n::text(if !is_live {
                    "voice.speech_recognition_ready"
                } else if microphone_ready {
                    "voice.microphone_permission_authorized"
                } else {
                    "voice.microphone_permission_required_2"
                })),
        )
        .when(state.input_device_picker_expanded(), |row| {
            row.child(expanded_device_options(theme, options, cx))
        })
}

fn expanded_device_options(theme: Theme, options: Div, cx: &mut Context<MultisoulApp>) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .pt(px(10.0))
        .border_t_1()
        .border_color(rgba(with_alpha(theme.text_primary, 0x18)))
        .child(options)
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(px(10.0))
                .child(
                    action_button(theme, ui::i18n::text("voice.refresh"), theme.accent_orange)
                        .id("settings-voice-input-device-refresh")
                        .debug_selector(|| "settings-voice-input-device-refresh".to_string())
                        .on_click(
                            cx.listener(MultisoulApp::on_voice_settings_refresh_input_devices),
                        ),
                )
                .child(
                    action_button(
                        theme,
                        ui::i18n::text("voice.use_system_default"),
                        theme.accent_cyan,
                    )
                    .id("settings-voice-input-device-clear")
                    .debug_selector(|| "settings-voice-input-device-clear".to_string())
                    .on_click(
                        cx.listener(MultisoulApp::on_voice_settings_use_default_input_device),
                    ),
                ),
        )
}

fn input_device_option(theme: Theme, label: String, selected: bool) -> Div {
    let accent = if selected {
        theme.accent_orange
    } else {
        theme.text_primary
    };
    div()
        .w_full()
        .max_w(px(520.0))
        .px(px(12.0))
        .py(px(9.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(accent, if selected { 0x90 } else { 0x28 })))
        .bg(rgba(with_alpha(accent, if selected { 0x18 } else { 0x06 })))
        .settings_font(TextRole::Body)
        .text_color(rgba(if selected {
            theme.accent_orange
        } else {
            theme.text_secondary
        }))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(with_alpha(theme.text_primary, 0x0c))))
        .child(label)
}
