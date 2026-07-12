pub(crate) use self::editable::VoiceSettingsInputs;
use self::interaction::interaction_group;
use self::model_catalog::model_catalog_row;
use self::provider::{agent_provider_advanced_rows, agent_provider_group};
use self::tts_catalog::tts_catalog_row;
use super::state::{VoiceSettingsField, VoiceSettingsState};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, ClickEvent, Context, Div, FontWeight, Window};
use ui::Theme;

mod editable;
mod interaction;
mod model_catalog;
mod provider;
mod tts_catalog;

pub(crate) fn voice_settings(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(20.0))
        .child(interaction_group(theme, state, cx))
        .child(
            settings_group(theme, "Speech Recognition")
                .child(input_device_row(theme, state, cx))
                .child(model_catalog_row(theme, state, cx)),
        )
        .child(agent_provider_group(theme, state, inputs, cx))
        .child(advanced_group(theme, state, inputs, cx))
        .child(settings_group(theme, "Spoken Reply").child(tts_catalog_row(theme, state, cx)))
        .children(status_message(theme, state))
}

impl MultisoulApp {
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
        self.persist_voice_settings_in_background(cx);
        cx.notify();
    }

    pub(crate) fn on_voice_settings_toggle_trace(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.toggle_trace();
        self.persist_voice_settings_in_background(cx);
        cx.notify();
    }

    pub(crate) fn on_voice_settings_toggle_advanced(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.toggle_advanced_expanded();
        cx.notify();
    }

    pub(crate) fn persist_voice_settings_in_background(&mut self, cx: &mut Context<Self>) {
        let writes = self.voice_settings.take_pending_writes();
        if writes.is_empty() {
            return;
        }
        let work = cx.background_executor().spawn(async move {
            for write in writes {
                write.execute().map_err(|error| error.to_string())?;
            }
            Ok::<(), String>(())
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                app.voice_settings.record_persistence_result(result);
                app.persist_voice_settings_in_background(cx);
                cx.notify();
            });
        })
        .detach();
    }
}

fn advanced_group(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let expanded = state.advanced_expanded();
    settings_group(theme, "Advanced")
        .child(
            setting_row(
                theme,
                "Diagnostics",
                "Endpoints, trace output and recovery details for advanced users",
            )
            .child(
                action_button(
                    theme,
                    if expanded { "Hide" } else { "Show" },
                    theme.accent_cyan,
                )
                .id("settings-voice-advanced-toggle")
                .debug_selector(|| "settings-voice-advanced-toggle".to_string())
                .on_click(cx.listener(MultisoulApp::on_voice_settings_toggle_advanced)),
            ),
        )
        .when(expanded, |group| {
            group
                .child(agent_provider_advanced_rows(theme, inputs, cx))
                .child(trace_row(theme, state, cx))
        })
}

pub(super) fn settings_group(theme: Theme, title: &'static str) -> Div {
    div()
        .w_full()
        .min_w(px(0.0))
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

pub(super) fn text_setting_row(
    theme: Theme,
    inputs: &VoiceSettingsInputs,
    field: VoiceSettingsField,
    title: &'static str,
    hint: &'static str,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let _ = cx;
    setting_row(theme, title, hint).child(inputs.input(field))
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
                " (System Default)"
            } else {
                ""
            }
        );
        options = options.child(
            input_device_option(theme, label, selected)
                .id(("settings-voice-input-device", index))
                .debug_selector(move || format!("settings-voice-input-device-{index}"))
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
                        .debug_selector(|| "settings-voice-input-device-refresh".to_string())
                        .on_click(
                            cx.listener(MultisoulApp::on_voice_settings_refresh_input_devices),
                        ),
                )
                .child(
                    action_button(theme, "Use system default", theme.accent_cyan)
                        .id("settings-voice-input-device-clear")
                        .debug_selector(|| "settings-voice-input-device-clear".to_string())
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
            .debug_selector(|| "settings-voice-trace-toggle".to_string())
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

pub(super) fn setting_row(theme: Theme, title: &'static str, hint: &'static str) -> Div {
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .items_start()
        .gap(px(10.0))
        .py(px(6.0))
        .child(setting_label(theme, title, hint))
}

pub(super) fn setting_label(theme: Theme, title: &'static str, hint: &'static str) -> Div {
    div()
        .w_full()
        .min_w(px(0.0))
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

/// Settings actions keep a GPUI `Div` because callers attach stable debug selectors after
/// construction; gpui-component 0.5.1's `Button` does not expose that test seam or this compact
/// accent variant. Keep this exception local instead of introducing another general button type.
pub(super) fn action_button(_theme: Theme, label: &'static str, accent: u32) -> Div {
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

pub(super) fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
