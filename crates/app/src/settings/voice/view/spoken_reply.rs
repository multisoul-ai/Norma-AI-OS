use super::tts_catalog::tts_catalog_row;
use super::{setting_row, settings_group, with_alpha};
use crate::app::MultisoulApp;
use crate::settings::voice::VoiceSettingsState;
use gpui::{div, prelude::*, px, rgba, Context, Div};
use ui::Theme;

pub(super) fn spoken_reply_group(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    settings_group(theme, "Spoken Reply")
        .child(toggle_row(
            theme,
            "Agent task announcements",
            "Speak the Agent name when a task completes or fails",
            state.values().agent_task_announcements_enabled(),
            "settings-agent-task-announcements-toggle",
            cx.listener(MultisoulApp::on_voice_settings_toggle_agent_task_announcements),
        ))
        .child(toggle_row(
            theme,
            "Voice-driven window inset",
            "Show the shared PCM-synchronized light while local speech is audible",
            state.values().voice_window_inset_enabled(),
            "settings-voice-window-inset-toggle",
            cx.listener(MultisoulApp::on_voice_settings_toggle_window_inset),
        ))
        .child(tts_catalog_row(theme, state, cx))
}

fn toggle_row(
    theme: Theme,
    title: &'static str,
    hint: &'static str,
    enabled: bool,
    selector: &'static str,
    listener: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> Div {
    setting_row(theme, title, hint).child(
        div()
            .id(selector)
            .debug_selector(move || selector.to_string())
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
            .on_click(listener),
    )
}
