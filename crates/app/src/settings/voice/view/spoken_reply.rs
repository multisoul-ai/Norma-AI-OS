use super::tts_provider::tts_provider_settings;
use super::{setting_label, settings_group, VoiceSettingsInputs};
use crate::app::MultisoulApp;
use crate::settings::voice::VoiceSettingsState;
use gpui::{div, prelude::*, px, rgba, Context, Div, KeyDownEvent};
use std::rc::Rc;
use ui::component::{switch::Switch, Sizable};
use ui::Theme;

pub(super) fn spoken_reply_group(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    settings_group(theme, ui::i18n::text("voice.spoken_replies"))
        .child(tts_provider_settings(theme, state, inputs, cx))
        .child(reply_preferences(theme, state, cx))
}

pub(super) fn reply_preferences(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(toggle_row(
            theme,
            ui::i18n::text("voice.agent_task_announcements"),
            ui::i18n::text("voice.announce_when_an_agent_task_finishes_or_needs_attention"),
            state.values().agent_task_announcements_enabled(),
            "settings-agent-task-announcements-toggle",
            cx.listener(MultisoulApp::on_voice_settings_toggle_agent_task_announcements),
        ))
        .child(toggle_row(
            theme,
            ui::i18n::text("voice.speech_glow"),
            ui::i18n::text("voice.show_a_subtle_window_glow_while_norma_speaks"),
            state.values().voice_window_inset_enabled(),
            "settings-voice-window-inset-toggle",
            cx.listener(MultisoulApp::on_voice_settings_toggle_window_inset),
        ))
}

pub(super) fn toggle_row(
    theme: Theme,
    title: &'static str,
    hint: &'static str,
    enabled: bool,
    selector: &'static str,
    listener: impl Fn(&bool, &mut gpui::Window, &mut gpui::App) + 'static,
) -> Div {
    let listener = Rc::new(listener);
    let mouse_listener = listener.clone();
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(16.0))
        .py(px(6.0))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .child(setting_label(theme, title, hint)),
        )
        .child(
            div()
                .id(selector)
                .debug_selector(move || selector.to_string())
                .flex_none()
                .tab_index(0)
                .rounded(px(5.0))
                .border_1()
                .border_color(rgba(0x00000000))
                .p(px(3.0))
                .hover(move |style| style.bg(rgba(theme.panel_high)))
                .focus(move |style| style.border_color(rgba(theme.accent_blue)))
                .on_key_down(move |event: &KeyDownEvent, window, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        listener(&!enabled, window, cx);
                        cx.stop_propagation();
                    }
                })
                .child(
                    Switch::new(selector)
                        .small()
                        .checked(enabled)
                        .on_click(move |next, window, cx| mouse_listener(next, window, cx)),
                ),
        )
}
