use super::{with_alpha, VoiceSettingsInputs};
use crate::{
    app::{LiveCallView, MultisoulApp},
    settings::{
        typography::{SettingsText, TextRole},
        voice::{VoiceEntryState, VoiceInteractionMode, VoiceSettingsField, VoiceSettingsState},
    },
};
use gpui::{div, prelude::*, px, rgba, Context, Div, SharedString, Window};
use ui::{
    component::{
        button::{Button, ButtonCustomVariant, ButtonVariants},
        Disableable,
    },
    Theme,
};

mod device;
mod diagnostics;

pub(super) fn gpt_live_settings(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    call: LiveCallView,
    shortcut: &str,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(connection(theme, state, inputs, &call, cx))
        .child(preferences(theme, state, shortcut, cx))
        .child(diagnostics::section(theme, state, &call, cx))
}

fn connection(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    call: &LiveCallView,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let configured = state.values().gpt_live_profile().credential_ref().is_some();
    let pending = state.credential_persistence_pending();
    let ready = state.readiness() == VoiceEntryState::Ready;
    let editing = state.gpt_live_key_expanded()
        || state.editing_field() == Some(VoiceSettingsField::GptLiveApiKey);
    let disabled = !call.active && !ready;
    section(theme)
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(16.0))
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .settings_text(TextRole::Headline, theme)
                                .child("GPT-Live"),
                        )
                        .child(helper(theme, ui::i18n::text("voice.live_settings_intro"))),
                )
                .child(
                    div()
                        .id("settings-live-call-toggle")
                        .debug_selector(|| "settings-live-call-toggle".into())
                        .flex_none()
                        .child(
                            Button::new("settings-live-connect-button")
                                .child(
                                    div()
                                        .when(disabled, |label| {
                                            label.text_color(rgba(theme.text_secondary))
                                        })
                                        .child(ui::i18n::text(if call.active {
                                            "voice.live_end_call"
                                        } else {
                                            "voice.live_connect"
                                        })),
                                )
                                .primary()
                                .disabled(disabled)
                                .on_click(cx.listener(move |app, _, window, cx| {
                                    if !disabled {
                                        app.toggle_gpt_live_voice(window, cx);
                                    }
                                })),
                        ),
                ),
        )
        .child(
            div()
                .id("settings-live-call-status")
                .debug_selector(|| "settings-live-call-status".into())
                .settings_text(TextRole::Helper, theme)
                .child(if call.status == ui::i18n::text("voice.live_name") {
                    ui::i18n::text("voice.live_disconnected").to_string()
                } else {
                    call.status.clone()
                }),
        )
        .when(call.active, |group| {
            group.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(live_action(
                        theme,
                        "settings-live-call-mute",
                        ui::i18n::text(if call.muted {
                            "voice.live_unmute"
                        } else {
                            "voice.live_mute"
                        }),
                        call.muted,
                        !call.can_mute,
                        cx,
                        |app, _, cx| app.toggle_gpt_live_mute(cx),
                    ))
                    .child(live_action(
                        theme,
                        "settings-live-call-output",
                        ui::i18n::text(if call.output_blocked {
                            "voice.live_resume_output"
                        } else {
                            "voice.live_stop_output"
                        }),
                        call.output_blocked,
                        !call.can_output,
                        cx,
                        |app, _, cx| app.toggle_gpt_live_output(cx),
                    )),
            )
        })
        .when_some(call.tools_error.clone(), |group, error| {
            group.child(
                helper(theme, error)
                    .id("settings-live-tools-error")
                    .text_color(rgba(theme.accent_orange)),
            )
        })
        .child(
            row(
                theme,
                "API Key",
                ui::i18n::text(if pending {
                    "voice.saving"
                } else if configured {
                    "voice.live_key_saved"
                } else {
                    "voice.api_key_required"
                }),
            )
            .when(configured, |row| {
                row.child(
                    live_action(
                        theme,
                        "settings-live-key-edit",
                        ui::i18n::text(if editing {
                            "voice.hide"
                        } else {
                            "voice.live_key_change"
                        }),
                        false,
                        pending,
                        cx,
                        move |app, window, cx| {
                            if editing {
                                app.focus_handle.focus(window);
                                app.cancel_voice_text_input(VoiceSettingsField::GptLiveApiKey, cx);
                                app.flush_voice_text_input_autosave(cx);
                                app.voice_settings_inputs.sync_field_from_state(
                                    VoiceSettingsField::GptLiveApiKey,
                                    &app.voice_settings,
                                    cx,
                                );
                            }
                            app.voice_settings.set_gpt_live_key_expanded(!editing);
                            cx.notify();
                        },
                    )
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation()),
                )
            }),
        )
        .when(
            !configured || editing || state.has_gpt_live_credential_retry(),
            |group| {
                group
                    .child(inputs.input(VoiceSettingsField::GptLiveApiKey))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(8.0))
                            .when(configured, |actions| {
                                actions.child(
                                    live_action(
                                        theme,
                                        "settings-live-key-clear",
                                        ui::i18n::text("common.remove"),
                                        false,
                                        pending,
                                        cx,
                                        |app, window, cx| {
                                            app.focus_handle.focus(window);
                                            app.cancel_voice_text_input(
                                                VoiceSettingsField::GptLiveApiKey,
                                                cx,
                                            );
                                            app.voice_settings.set_gpt_live_key_expanded(false);
                                            app.stop_gpt_live_voice(cx);
                                            app.voice_settings.clear_gpt_live_credential();
                                            app.voice_settings_inputs.sync_field_from_state(
                                                VoiceSettingsField::GptLiveApiKey,
                                                &app.voice_settings,
                                                cx,
                                            );
                                            app.persist_voice_settings_in_background(cx);
                                            cx.notify();
                                        },
                                    )
                                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    }),
                                )
                            })
                            .when(state.has_gpt_live_credential_retry(), |actions| {
                                actions.child(live_action(
                                    theme,
                                    "settings-live-key-retry",
                                    ui::i18n::text("voice.live_retry_key"),
                                    false,
                                    pending,
                                    cx,
                                    |app, _, cx| {
                                        app.stop_gpt_live_voice(cx);
                                        app.voice_settings.retry_gpt_live_credential_change();
                                        app.persist_voice_settings_in_background(cx);
                                        cx.notify();
                                    },
                                ))
                            }),
                    )
            },
        )
}

fn preferences(
    theme: Theme,
    state: &VoiceSettingsState,
    shortcut: &str,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let mode = state.values().gpt_live_interaction_mode();
    let mut modes = div().flex().gap(px(0.0));
    for (value, selector, label) in [
        (
            VoiceInteractionMode::ContinuousListening,
            "settings-live-mode-realtime",
            "voice.realtime_conversation",
        ),
        (
            VoiceInteractionMode::SingleVoiceCommand,
            "settings-live-mode-hold",
            "voice.push_to_talk",
        ),
    ] {
        modes = modes.child(live_action(
            theme,
            selector,
            ui::i18n::text(label),
            mode == value,
            false,
            cx,
            move |app, _, cx| app.select_gpt_live_interaction_mode(value, cx),
        ));
    }
    let selected = state.values().gpt_live_profile().voice();
    let mut voices = div().flex().gap(px(8.0));
    for (id, selector, label) in [
        ("marin", "settings-live-voice-marin", "Marin"),
        ("cedar", "settings-live-voice-cedar", "Cedar"),
    ] {
        voices = voices.child(live_action(
            theme,
            selector,
            label,
            selected == id,
            false,
            cx,
            move |app, _, cx| {
                app.voice_settings.select_gpt_live_voice(id);
                app.persist_voice_settings_in_background(cx);
                cx.notify();
            },
        ));
    }
    section(theme)
        .child(
            div()
                .settings_text(TextRole::Headline, theme)
                .child(ui::i18n::text("voice.live_preferences")),
        )
        .child(
            row(
                theme,
                ui::i18n::text("shortcuts.voice_interaction"),
                &ui::i18n::format(
                    if mode == VoiceInteractionMode::ContinuousListening {
                        "voice.live_realtime_shortcut_short"
                    } else {
                        "voice.live_hold_shortcut_short"
                    },
                    &[("shortcut", shortcut.to_string())],
                ),
            )
            .child(modes),
        )
        .child(
            row(
                theme,
                ui::i18n::text("voice.live_voice_label"),
                ui::i18n::text("voice.live_voice_next_call"),
            )
            .child(voices),
        )
        .child(device::row(theme, state, cx))
}

fn section(theme: Theme) -> Div {
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .p(px(16.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.text_primary, 0x18)))
        .bg(rgba(theme.panel_high))
}

fn row(theme: Theme, title: &str, hint: &str) -> Div {
    div()
        .w_full()
        .flex()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .py(px(6.0))
        .child(
            div()
                .flex_1()
                .min_w(px(160.0))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .settings_text(TextRole::RowTitle, theme)
                        .child(title.to_string()),
                )
                .child(helper(theme, hint.to_string())),
        )
}

fn helper(theme: Theme, text: impl Into<SharedString>) -> Div {
    div()
        .settings_text(TextRole::Helper, theme)
        .child(text.into())
}

fn live_action(
    theme: Theme,
    selector: &'static str,
    label: impl Into<SharedString>,
    selected: bool,
    disabled: bool,
    cx: &Context<MultisoulApp>,
    action: impl Fn(&mut MultisoulApp, &mut Window, &mut Context<MultisoulApp>) + 'static,
) -> gpui::Stateful<Div> {
    let variant = ButtonCustomVariant::new(cx)
        .color(
            rgba(with_alpha(
                theme.accent_blue,
                if selected { 0x26 } else { 0x08 },
            ))
            .into(),
        )
        .foreground(rgba(theme.text_primary).into())
        .border(
            rgba(with_alpha(
                if selected {
                    theme.accent_blue
                } else {
                    theme.text_primary
                },
                if selected { 0x88 } else { 0x28 },
            ))
            .into(),
        )
        .hover(rgba(with_alpha(theme.accent_blue, 0x36)).into())
        .active(rgba(with_alpha(theme.accent_blue, 0x48)).into());
    let label = label.into();
    div()
        .id(selector)
        .debug_selector(move || selector.into())
        .flex_none()
        .child(
            Button::new(selector)
                .child(
                    div()
                        .when(disabled, |label| {
                            label.text_color(rgba(theme.text_secondary))
                        })
                        .child(if selected {
                            format!("✓ {label}")
                        } else {
                            label.to_string()
                        }),
                )
                .compact()
                .custom(variant)
                .disabled(disabled)
                .on_click(cx.listener(move |app, _, window, cx| {
                    if !disabled {
                        action(app, window, cx);
                    }
                })),
        )
}

#[cfg(test)]
#[path = "../../../../tests/unit/gpt_live_settings_material_tests.rs"]
mod material_tests;
