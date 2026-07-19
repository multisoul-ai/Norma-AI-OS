use super::tts_provider::tts_provider_settings;
use super::{action_button, setting_row, settings_group, with_alpha, VoiceSettingsInputs};
use crate::app::MultisoulApp;
use crate::settings::voice::VoiceSettingsState;
use gpui::{div, prelude::*, px, rgba, Context, Div};
use ui::Theme;

pub(super) fn spoken_reply_group(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    settings_group(theme, "Spoken Reply")
        .child(tts_provider_settings(theme, state, inputs, cx))
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
            "Show the shared PCM-synchronized light while speech is audible",
            state.values().voice_window_inset_enabled(),
            "settings-voice-window-inset-toggle",
            cx.listener(MultisoulApp::on_voice_settings_toggle_window_inset),
        ))
        .child(selected_voice_row(theme, state, cx))
}

fn selected_voice_row(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let local_active = state
        .values()
        .active_tts_provider_id()
        .is_some_and(|provider| provider.as_str() == tts::LOCAL_TTS_PROVIDER_ID);
    let selection = local_active
        .then(|| state.values().tts_model_id())
        .flatten()
        .and_then(|model_id| tts::builtin_model_catalog().find(model_id).cloned())
        .and_then(|model| {
            state
                .values()
                .tts_voice_id()
                .and_then(|voice_id| model.voice(voice_id).cloned())
                .map(|voice| (model.id().clone(), voice))
        });
    let minimax_label = state
        .values()
        .active_tts_provider_id()
        .is_some_and(|provider| provider.as_str() == tts::MINIMAX_TTS_PROVIDER_ID)
        .then(|| {
            state
                .values()
                .minimax_tts_voice_id()
                .map(|voice| format!("MiniMax · {}", voice.as_str()))
        })
        .flatten();
    let label = selection
        .as_ref()
        .map(|(_, voice)| voice.display_name().to_string())
        .or(minimax_label)
        .unwrap_or_else(|| "No voice selected".to_string());
    let mut row = setting_row(
        theme,
        "Voice",
        "Active Provider voice used for replies, Preview and task announcements",
    )
    .child(div().text_color(rgba(theme.text_primary)).child(label));
    if let Some((model_id, voice)) = selection {
        let voice_id = voice.id().clone();
        row = row.child(
            action_button(theme, "Preview", theme.accent_blue)
                .id("settings-voice-preview-selected")
                .debug_selector(|| "settings-voice-preview-selected".to_string())
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.preview_tts_voice(model_id.clone(), voice_id.clone(), cx)
                })),
        );
    }
    row
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
