use super::components::VoiceAction;
use super::{settings_group, with_alpha};
use crate::app::MultisoulApp;
use crate::settings::typography::{control_label, SettingsText, TextRole};
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
    settings_group(theme, ui::i18n::text("shortcuts.voice_interaction"))
        .child(compact_interaction_row(
            theme,
            ui::i18n::text("voice.mode"),
            ui::i18n::format(
                "voice.choose_how_voice_shortcut_label_starts_voice",
                &[("voice_shortcut_label", (voice_shortcut_label).to_string())],
            ),
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(px(8.0))
                .child(
                    mode_option(
                        theme,
                        "settings-voice-mode-dictation",
                        ui::i18n::text("voice.single_voice_command"),
                        mode == VoiceInteractionMode::SingleVoiceCommand,
                        cx,
                    )
                    .on_activate(cx.listener(|app, _, _, cx| {
                        app.stop_continuous_listening(cx);
                        app.voice_settings
                            .select_interaction_mode(VoiceInteractionMode::SingleVoiceCommand);
                        if app.voice_settings.values().asr_provider() == asr::AsrProviderId::LocalA1
                        {
                            app.refresh_a1_requirement(cx);
                        }
                        app.persist_voice_settings_in_background(cx);
                        cx.notify();
                    })),
                )
                .child(
                    mode_option(
                        theme,
                        "settings-voice-mode-continuous",
                        ui::i18n::text("voice.continuous_listening"),
                        mode == VoiceInteractionMode::ContinuousListening,
                        cx,
                    )
                    .on_activate(cx.listener(|app, _, _, cx| {
                        app.voice.stop_for_model_switch();
                        app.release_a1_runtime(cx);
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

pub(super) fn doubao_shortcut_hint(state: &VoiceSettingsState, shortcut: &str) -> String {
    ui::i18n::format(
        if state.values().doubao_interaction_mode() == VoiceInteractionMode::ContinuousListening {
            "voice.doubao_realtime_shortcut"
        } else {
            "voice.doubao_hold_shortcut"
        },
        &[("shortcut", shortcut.to_string())],
    )
}

pub(super) fn doubao_interaction_group(
    theme: Theme,
    state: &VoiceSettingsState,
    shortcut: &str,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let mode = state.values().doubao_interaction_mode();
    settings_group(theme, ui::i18n::text("shortcuts.voice_interaction")).child(
        compact_interaction_row(
            theme,
            ui::i18n::text("voice.mode"),
            doubao_shortcut_hint(state, shortcut),
            div()
                .flex()
                .flex_wrap()
                .gap(px(8.0))
                .child(
                    mode_option(
                        theme,
                        "settings-doubao-mode-realtime",
                        ui::i18n::text("voice.realtime_conversation"),
                        mode == VoiceInteractionMode::ContinuousListening,
                        cx,
                    )
                    .on_activate(cx.listener(|app, _, _, cx| {
                        app.stop_doubao_voice(cx);
                        app.voice_settings.select_doubao_interaction_mode(
                            VoiceInteractionMode::ContinuousListening,
                        );
                        app.persist_voice_settings_in_background(cx);
                        cx.notify();
                    })),
                )
                .child(
                    mode_option(
                        theme,
                        "settings-doubao-mode-hold",
                        ui::i18n::text("voice.push_to_talk"),
                        mode == VoiceInteractionMode::SingleVoiceCommand,
                        cx,
                    )
                    .on_activate(cx.listener(|app, _, _, cx| {
                        app.stop_doubao_voice(cx);
                        app.voice_settings.select_doubao_interaction_mode(
                            VoiceInteractionMode::SingleVoiceCommand,
                        );
                        app.persist_voice_settings_in_background(cx);
                        cx.notify();
                    })),
                ),
        ),
    )
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
        ui::i18n::text("voice.shortcut"),
        ui::i18n::format(
            "voice.shortcut_hold_or_toggle",
            &[("shortcut", (shortcut).to_string())],
        ),
        div()
            .flex()
            .flex_row()
            .gap(px(8.0))
            .child(
                mode_option(
                    theme,
                    "settings-voice-trigger-hold",
                    ui::i18n::text("voice.hold"),
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
                    ui::i18n::text("voice.toggle"),
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
                .child(div().settings_text(TextRole::RowTitle, theme).child(title))
                .child(div().settings_text(TextRole::Helper, theme).child(hint)),
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
        .child(control_label(if selected {
            format!("✓ {label}")
        } else {
            label.to_string()
        }))
        .compact()
        .custom(variant)
}
