use super::{setting_row, settings_group, with_alpha};
use crate::app::MultisoulApp;
use crate::settings::voice::{DictationTrigger, VoiceInteractionMode, VoiceSettingsState};
use gpui::{div, prelude::*, px, rgba, Context, Div};
use ui::Theme;

pub(super) fn interaction_group(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let mode = state.values().interaction_mode();
    settings_group(theme, "Voice Interaction")
        .child(
            setting_row(
                theme,
                "Mode",
                "⌘E starts a single voice command or continuous Agent conversation",
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.0))
                    .child(
                        mode_option(
                            theme,
                            "Single Voice Command",
                            mode == VoiceInteractionMode::SingleVoiceCommand,
                        )
                        .id("settings-voice-mode-dictation")
                        .debug_selector(|| "settings-voice-mode-dictation".to_string())
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.stop_continuous_listening(cx);
                            app.voice_settings
                                .select_interaction_mode(VoiceInteractionMode::SingleVoiceCommand);
                            app.persist_voice_settings_in_background(cx);
                            cx.notify();
                        })),
                    )
                    .child(
                        mode_option(
                            theme,
                            "Continuous Listening",
                            mode == VoiceInteractionMode::ContinuousListening,
                        )
                        .id("settings-voice-mode-continuous")
                        .debug_selector(|| "settings-voice-mode-continuous".to_string())
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.voice.stop_for_model_switch();
                            app.voice_settings
                                .select_interaction_mode(VoiceInteractionMode::ContinuousListening);
                            app.persist_voice_settings_in_background(cx);
                            cx.notify();
                        })),
                    ),
            ),
        )
        .when(mode == VoiceInteractionMode::SingleVoiceCommand, |group| {
            group.child(dictation_trigger_row(theme, state, cx))
        })
}

fn dictation_trigger_row(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let trigger = state.values().dictation_trigger();
    setting_row(
        theme,
        "Single Voice Command Trigger",
        "Hold ends on shortcut release; Toggle ends on the next ⌘E press",
    )
    .child(
        div()
            .flex()
            .flex_row()
            .gap(px(8.0))
            .child(
                mode_option(theme, "Hold", trigger == DictationTrigger::Hold)
                    .id("settings-voice-trigger-hold")
                    .debug_selector(|| "settings-voice-trigger-hold".to_string())
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.voice_settings
                            .select_dictation_trigger(DictationTrigger::Hold);
                        app.persist_voice_settings_in_background(cx);
                        cx.notify();
                    })),
            )
            .child(
                mode_option(theme, "Toggle", trigger == DictationTrigger::Toggle)
                    .id("settings-voice-trigger-toggle")
                    .debug_selector(|| "settings-voice-trigger-toggle".to_string())
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.voice_settings
                            .select_dictation_trigger(DictationTrigger::Toggle);
                        app.persist_voice_settings_in_background(cx);
                        cx.notify();
                    })),
            ),
    )
}

/// This is a two-state segmented option, not a general button. The pinned component Button lacks
/// the selected-state theme plus stable debug-selector seam required by the Settings e2e tests.
fn mode_option(theme: Theme, label: &'static str, selected: bool) -> Div {
    let accent = if selected {
        theme.accent_orange
    } else {
        theme.edge_highlight
    };
    div()
        .px(px(14.0))
        .py(px(9.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(accent, if selected { 0x90 } else { 0x50 })))
        .bg(rgba(with_alpha(
            if selected {
                theme.accent_orange
            } else {
                theme.panel_high
            },
            if selected { 0x20 } else { 0xa8 },
        )))
        .text_color(rgba(if selected {
            theme.accent_orange
        } else {
            theme.text_secondary
        }))
        .cursor_pointer()
        .child(label)
}
