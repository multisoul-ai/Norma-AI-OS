use super::components::VoiceAction;
use super::{settings_group, with_alpha};
use crate::app::MultisoulApp;
use crate::settings::voice::{DictationTrigger, VoiceInteractionMode, VoiceSettingsState};
use gpui::{div, prelude::*, px, rgba, Context, Div};
use ui::component::button::{Button, ButtonCustomVariant, ButtonVariants};
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
                .flex_wrap()
                .gap(px(8.0))
                .child(
                    mode_option(
                        theme,
                        "settings-voice-mode-dictation",
                        "Single Voice Command",
                        mode == VoiceInteractionMode::SingleVoiceCommand,
                        cx,
                    )
                    .on_activate(cx.listener(|app, _, _, cx| {
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
                        "settings-voice-mode-continuous",
                        "Continuous Listening",
                        mode == VoiceInteractionMode::ContinuousListening,
                        cx,
                    )
                    .on_activate(cx.listener(|app, _, _, cx| {
                        app.voice.stop_for_model_switch();
                        app.voice_settings
                            .select_interaction_mode(VoiceInteractionMode::ContinuousListening);
                        app.persist_voice_settings_in_background(cx);
                        cx.notify();
                    })),
                ),
        ))
        .when(mode == VoiceInteractionMode::SingleVoiceCommand, |group| {
            group.child(dictation_trigger_row(
                theme,
                state,
                voice_shortcut_label,
                cx,
            ))
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
                mode_option(
                    theme,
                    "settings-voice-trigger-hold",
                    "Hold",
                    trigger == DictationTrigger::Hold,
                    cx,
                )
                .on_activate(cx.listener(|app, _, _, cx| {
                    app.voice_settings
                        .select_dictation_trigger(DictationTrigger::Hold);
                    app.persist_voice_settings_in_background(cx);
                    cx.notify();
                })),
            )
            .child(
                mode_option(
                    theme,
                    "settings-voice-trigger-toggle",
                    "Toggle",
                    trigger == DictationTrigger::Toggle,
                    cx,
                )
                .on_activate(cx.listener(|app, _, _, cx| {
                    app.voice_settings
                        .select_dictation_trigger(DictationTrigger::Toggle);
                    app.persist_voice_settings_in_background(cx);
                    cx.notify();
                })),
            ),
    )
}

fn compact_interaction_row(theme: Theme, title: &'static str, hint: String, control: Div) -> Div {
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap(px(16.0))
        .py(px(5.0))
        .child(
            div()
                .flex_1()
                .min_w(px(150.0))
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

pub(super) fn mode_option(
    theme: Theme,
    selector: &'static str,
    label: &'static str,
    selected: bool,
    cx: &Context<MultisoulApp>,
) -> Button {
    let variant = ButtonCustomVariant::new(cx)
        .color(
            rgba(with_alpha(
                theme.accent_blue,
                if selected { 0x18 } else { 0x06 },
            ))
            .into(),
        )
        .foreground(
            rgba(if selected {
                theme.accent_blue
            } else {
                theme.text_secondary
            })
            .into(),
        )
        .border(
            rgba(with_alpha(
                if selected {
                    theme.accent_blue
                } else {
                    theme.text_secondary
                },
                if selected { 0x70 } else { 0x28 },
            ))
            .into(),
        )
        .hover(rgba(with_alpha(theme.accent_blue, 0x28)).into())
        .active(rgba(with_alpha(theme.accent_blue, 0x38)).into());
    Button::new(selector)
        .debug_selector(move || selector.to_string())
        .label(if selected {
            format!("✓ {label}")
        } else {
            label.to_string()
        })
        .compact()
        .custom(variant)
}
