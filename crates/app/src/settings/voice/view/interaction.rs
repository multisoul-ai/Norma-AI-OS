use super::{settings_group, with_alpha};
use crate::app::MultisoulApp;
use crate::settings::voice::{DictationTrigger, VoiceInteractionMode, VoiceSettingsState};
use gpui::{div, prelude::*, px, rgba, Context, Div};
use ui::Theme;

pub(super) fn interaction_group(
    theme: Theme,
    state: &VoiceSettingsState,
    voice_shortcut_label: &str,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let mode = state.values().interaction_mode();
    settings_group(theme, "Voice Interaction")
        .child(compact_interaction_row(
            theme,
            "Mode",
            format!("Choose how {voice_shortcut_label} starts Voice"),
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
        ))
        .when(mode == VoiceInteractionMode::SingleVoiceCommand, |group| {
            group.child(dictation_trigger_row(theme, state, voice_shortcut_label, cx))
        })
}

fn dictation_trigger_row(
    theme: Theme,
    state: &VoiceSettingsState,
    shortcut: &str,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let trigger = state.values().dictation_trigger();
    compact_interaction_row(
        theme,
        "Shortcut",
        format!("{shortcut} · Hold or toggle"),
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

fn compact_interaction_row(
    theme: Theme,
    title: &'static str,
    hint: String,
    control: Div,
) -> Div {
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(16.0))
        .py(px(5.0))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(rgba(theme.text_primary))
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgba(theme.text_tertiary))
                        .child(hint),
                ),
        )
        .child(control.flex_none())
}

/// This is a two-state segmented option, not a general button. The pinned component Button lacks
/// the selected-state theme plus stable debug-selector seam required by the Settings e2e tests.
fn mode_option(theme: Theme, label: &'static str, selected: bool) -> Div {
    let accent = if selected {
        theme.accent_orange
    } else {
        theme.text_primary
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
                theme.text_primary
            },
            if selected { 0x20 } else { 0x06 },
        )))
        .text_color(rgba(if selected {
            theme.accent_orange
        } else {
            theme.text_secondary
        }))
        .cursor_pointer()
        .child(label)
}
