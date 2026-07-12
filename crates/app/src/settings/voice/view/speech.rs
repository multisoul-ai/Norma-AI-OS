use super::{action_button, setting_label, setting_row, value_pill, with_alpha};
use crate::app::MultisoulApp;
use crate::settings::voice::state::VoiceSettingsState;
use gpui::{div, prelude::*, px, rgba, Context, Div};
use ui::Theme;

pub(super) fn model_dir_row(theme: Theme, state: &VoiceSettingsState, cx: &mut Context<MultisoulApp>) -> Div {
    let (path, ok) = model_dir_display(state);
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .py(px(6.0))
        .child(setting_label(
            theme,
            "Paraformer Model Directory",
            "Local sherpa-onnx model used for streaming speech recognition",
        ))
        .child(
            value_pill(theme, path, false)
                .id("settings-voice-model-dir-path")
                .w_full()
                .max_w(px(520.0))
                .text_color(rgba(if ok {
                    theme.text_secondary
                } else {
                    theme.destructive
                })),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(px(10.0))
                .child(
                    action_button(theme, "Choose...", theme.accent_orange)
                        .id("settings-voice-model-dir-choose")
                        .on_click(cx.listener(MultisoulApp::on_voice_settings_choose_model_dir)),
                )
                .child(
                    action_button(theme, "Use default", theme.accent_cyan)
                        .id("settings-voice-model-dir-default")
                        .on_click(
                            cx.listener(MultisoulApp::on_voice_settings_use_default_model_dir),
                        ),
                ),
        )
}
pub(super) fn input_device_row(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let selected_id = state.values().input_device_id();
    let mut options = div().flex().flex_col().gap(px(8.0)).child(
        input_device_option(theme, "System Default".to_string(), selected_id.is_none())
            .id("settings-voice-input-device-default")
            .on_click(cx.listener(MultisoulApp::on_voice_settings_use_default_input_device)),
    );

    for (index, device) in state.input_devices().iter().enumerate() {
        let id = device.id.clone();
        let selected = selected_id == Some(device.id.as_str());
        let label = format!(
            "{}{}",
            device.name,
            if device.is_default {
                " (System Default)"
            } else {
                ""
            }
        );
        options = options.child(
            input_device_option(theme, label, selected)
                .id(("settings-voice-input-device", index))
                .on_click(cx.listener(move |app, _, _window, cx| {
                    app.voice_settings.select_input_device(Some(id.clone()));
                    app.persist_voice_settings_in_background(cx);
                    cx.notify();
                })),
        );
    }

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .py(px(6.0))
        .child(setting_label(
            theme,
            "Microphone Input",
            "Select the microphone used for speech recognition",
        ))
        .child(options)
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(px(10.0))
                .child(
                    action_button(theme, "Refresh", theme.accent_orange)
                        .id("settings-voice-input-device-refresh")
                        .on_click(
                            cx.listener(MultisoulApp::on_voice_settings_refresh_input_devices),
                        ),
                )
                .child(
                    action_button(theme, "Use system default", theme.accent_cyan)
                        .id("settings-voice-input-device-clear")
                        .on_click(
                            cx.listener(MultisoulApp::on_voice_settings_use_default_input_device),
                        ),
                ),
        )
}

pub(super) fn trace_row(theme: Theme, state: &VoiceSettingsState, cx: &mut Context<MultisoulApp>) -> Div {
    let enabled = state.values().trace_enabled();
    setting_row(
        theme,
        "Full prompt/response trace",
        "Logs complete voice prompts and model responses; may include spoken content",
    )
    .child(
        div()
            .id("settings-voice-trace-toggle")
            .w(px(54.0))
            .h(px(30.0))
            .rounded(px(15.0))
            .border_1()
            .border_color(rgba(with_alpha(
                if enabled {
                    theme.accent_orange
                } else {
                    theme.edge_highlight
                },
                0x70,
            )))
            .bg(rgba(with_alpha(
                if enabled {
                    theme.accent_orange
                } else {
                    theme.panel_high
                },
                if enabled { 0x44 } else { 0xa8 },
            )))
            .cursor_pointer()
            .child(
                div()
                    .mt(px(3.0))
                    .ml(px(if enabled { 27.0 } else { 3.0 }))
                    .w(px(22.0))
                    .h(px(22.0))
                    .rounded(px(11.0))
                    .bg(rgba(if enabled {
                        theme.accent_orange
                    } else {
                        theme.text_secondary
                    })),
            )
            .on_click(cx.listener(MultisoulApp::on_voice_settings_toggle_trace)),
    )
}

fn input_device_option(theme: Theme, label: String, selected: bool) -> Div {
    let accent = if selected {
        theme.accent_orange
    } else {
        theme.edge_highlight
    };
    div()
        .w_full()
        .max_w(px(520.0))
        .px(px(12.0))
        .py(px(9.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(accent, if selected { 0x90 } else { 0x45 })))
        .bg(rgba(with_alpha(
            if selected {
                theme.accent_orange
            } else {
                theme.panel_high
            },
            if selected { 0x18 } else { 0xa8 },
        )))
        .text_size(px(13.0))
        .text_color(rgba(if selected {
            theme.accent_orange
        } else {
            theme.text_secondary
        }))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(with_alpha(theme.panel_high, 0xd0))))
        .child(label)
}

fn model_dir_display(state: &VoiceSettingsState) -> (String, bool) {
    if let Some(path) = state.values().paraformer_model_dir() {
        return (
            format!(
                "{}{}",
                path.display(),
                if path.exists() { "" } else { " (missing)" }
            ),
            path.exists(),
        );
    }

    match voice_sherpa::resolve_model_dir() {
        Ok(path) => (
            format!(
                "{}{}",
                path.display(),
                if path.exists() { "" } else { " (missing)" }
            ),
            path.exists(),
        ),
        Err(error) => (error.to_string(), false),
    }
}
