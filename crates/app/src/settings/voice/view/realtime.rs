use super::{setting_row, settings_group, text_setting_row, with_alpha};
use crate::app::MultisoulApp;
use crate::settings::voice::state::{VoiceSettingsField, VoiceSettingsState};
use gpui::{div, prelude::*, px, rgba, ClickEvent, Context, Div, Window};
use ui::Theme;

pub(super) fn realtime_group(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    settings_group(theme, "Realtime conversation")
        .child(realtime_toggle_row(theme, state, cx))
        .child(text_setting_row(
            theme,
            state,
            VoiceSettingsField::OpenAiApiKey,
            "OpenAI API Key",
            "Stored locally for Always Listening startup validation",
            cx,
        ))
        .child(text_setting_row(
            theme,
            state,
            VoiceSettingsField::RealtimeModel,
            "Realtime Model",
            "OpenAI realtime model used by the listening shell",
            cx,
        ))
        .child(text_setting_row(
            theme,
            state,
            VoiceSettingsField::CardPlannerModel,
            "Card Planner Model",
            "Model reserved for converting realtime intent into card actions",
            cx,
        ))
        .child(text_setting_row(
            theme,
            state,
            VoiceSettingsField::SidecarPath,
            "Sidecar Path",
            "Optional local sidecar executable path for a future realtime bridge",
            cx,
        ))
}

impl MultisoulApp {
    pub(crate) fn on_voice_settings_toggle_realtime(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let was_enabled = self.voice_settings.values().realtime_enabled();
        self.voice_settings.toggle_realtime_enabled();
        if was_enabled {
            self.stop_always_listening_shell(cx);
            return;
        }
        cx.notify();
    }
}

fn realtime_toggle_row(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let enabled = state.values().realtime_enabled();
    setting_row(
        theme,
        "Enable Always Listening",
        "Use the bottom voice bar as a realtime conversation shell",
    )
    .child(
        div()
            .id("settings-voice-realtime-toggle")
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
            .on_click(cx.listener(MultisoulApp::on_voice_settings_toggle_realtime)),
    )
}
