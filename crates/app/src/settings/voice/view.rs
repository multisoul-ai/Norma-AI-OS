use self::realtime::realtime_group;
use super::state::{VoiceSettingsField, VoiceSettingsState};
use crate::app::MultisoulApp;
use gpui::{
    div, prelude::*, px, rgba, ClickEvent, Context, Div, FontWeight, PathPromptOptions, Window,
};
use ui::Theme;

mod realtime;

pub(crate) fn voice_settings(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(20.0))
        .child(
            settings_group(theme, "Speech Recognition")
                .child(model_dir_row(theme, state, cx))
                .child(input_device_row(theme, state, cx)),
        )
        .child(
            settings_group(theme, "Command Planning")
                .child(text_setting_row(
                    theme,
                    state,
                    VoiceSettingsField::OllamaEndpoint,
                    "Ollama Endpoint",
                    "Local endpoint used by planner and transcript refine",
                    cx,
                ))
                .child(text_setting_row(
                    theme,
                    state,
                    VoiceSettingsField::PlannerModel,
                    "Planner Model",
                    "Ollama model that converts voice transcripts into card commands",
                    cx,
                ))
                .child(text_setting_row(
                    theme,
                    state,
                    VoiceSettingsField::RefineModel,
                    "Refine Model",
                    "Ollama model that minimally repairs transcript wording before planning",
                    cx,
                )),
        )
        .child(realtime_group(theme, state, cx))
        .child(settings_group(theme, "Advanced").child(trace_row(theme, state, cx)))
        .children(status_message(theme, state))
}

impl MultisoulApp {
    pub(crate) fn on_voice_settings_edit(
        &mut self,
        field: VoiceSettingsField,
        _event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.begin_edit(field);
        self.focus_handle.focus(window);
        cx.notify();
    }

    pub(crate) fn on_voice_settings_choose_model_dir(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose Paraformer Model Directory".into()),
        });

        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await {
                if let Some(path) = paths.into_iter().next() {
                    this.update(cx, |app, cx| {
                        app.voice_settings.set_paraformer_model_dir(path);
                        cx.notify();
                    })?;
                }
            }
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }

    pub(crate) fn on_voice_settings_use_default_model_dir(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.clear_paraformer_model_dir();
        cx.notify();
    }

    pub(crate) fn on_voice_settings_refresh_input_devices(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.refresh_input_devices();
        cx.notify();
    }

    pub(crate) fn on_voice_settings_use_default_input_device(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.select_input_device(None);
        cx.notify();
    }

    pub(crate) fn on_voice_settings_toggle_trace(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.toggle_trace();
        cx.notify();
    }
}

fn settings_group(theme: Theme, title: &'static str) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .pt(px(20.0))
        .border_t_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x24)))
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(theme.accent_cyan))
                .child(title),
        )
}

fn text_setting_row(
    theme: Theme,
    state: &VoiceSettingsState,
    field: VoiceSettingsField,
    title: &'static str,
    hint: &'static str,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    setting_row(theme, title, hint).child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(10.0))
            .child(value_pill(
                theme,
                state.row_value(field),
                state.is_editing(field),
            ))
            .child(
                action_button(theme, "Edit", theme.accent_orange)
                    .id(("settings-voice-edit", field.index()))
                    .on_click(cx.listener(move |app, event, window, cx| {
                        app.on_voice_settings_edit(field, event, window, cx);
                    })),
            ),
    )
}

fn model_dir_row(theme: Theme, state: &VoiceSettingsState, cx: &mut Context<MultisoulApp>) -> Div {
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

fn input_device_row(
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

fn trace_row(theme: Theme, state: &VoiceSettingsState, cx: &mut Context<MultisoulApp>) -> Div {
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

fn setting_row(theme: Theme, title: &'static str, hint: &'static str) -> Div {
    div()
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(24.0))
        .py(px(6.0))
        .child(setting_label(theme, title, hint))
}

fn setting_label(theme: Theme, title: &'static str, hint: &'static str) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .text_size(px(17.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(theme.text_primary))
                .child(title),
        )
        .child(
            div()
                .text_size(px(14.0))
                .text_color(rgba(theme.text_tertiary))
                .child(hint),
        )
}

fn value_pill(theme: Theme, value: String, editing: bool) -> Div {
    div()
        .max_w(px(300.0))
        .overflow_hidden()
        .truncate()
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(
            if editing {
                theme.accent_orange
            } else {
                theme.edge_highlight
            },
            0x60,
        )))
        .bg(rgba(with_alpha(theme.panel_high, 0xb8)))
        .text_size(px(13.0))
        .text_color(rgba(theme.text_secondary))
        .child(value)
}

fn action_button(_theme: Theme, label: &'static str, accent: u32) -> Div {
    div()
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(accent, 0x70)))
        .bg(rgba(with_alpha(accent, 0x18)))
        .text_size(px(13.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba(accent))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(with_alpha(accent, 0x28))))
        .child(label)
}

fn status_message(theme: Theme, state: &VoiceSettingsState) -> Vec<Div> {
    let Some((message, is_error)) = state.status() else {
        return Vec::new();
    };
    vec![div()
        .text_size(px(13.0))
        .text_color(rgba(if is_error {
            theme.destructive
        } else {
            theme.accent_cyan
        }))
        .child(message.to_string())]
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

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
