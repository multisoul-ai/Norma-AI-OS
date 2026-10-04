use super::*;

pub(super) fn trace_row(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
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
