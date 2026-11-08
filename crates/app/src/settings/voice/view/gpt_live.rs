//! GPT-Live settings: one column of configuration; calls start and end with the voice shortcut.
use super::{with_alpha, VoiceSettingsInputs};
use crate::{
    app::{LiveCallView, MultisoulApp},
    settings::{
        typography::{SettingsText, TextRole},
        voice::{VoiceInteractionMode, VoiceSettingsState},
    },
};
use gpui::{div, prelude::*, px, rgba, svg, Context, Div, SharedString, Window};
use ui::{
    component::{
        button::{Button, ButtonCustomVariant, ButtonVariants},
        Disableable,
    },
    Theme,
};

mod controls;
mod device;
mod diagnostics;
mod key;

use controls::{segmented, voice_option};

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
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .when(call.active, |page| {
            page.child(call_controls(theme, &call, cx))
        })
        .children(alert(theme, state, &call, shortcut, cx))
        .child(key::key_card(theme, state, inputs, cx))
        .child(section_title(theme))
        .child(preferences(theme, state, shortcut, cx))
        .child(diagnostics::section(theme, state, &call, cx))
        .child(footer(theme, state))
}

/// Last-call failure summary; the raw error stays in diagnostics.
fn alert(
    theme: Theme,
    state: &VoiceSettingsState,
    call: &LiveCallView,
    shortcut: &str,
    cx: &mut Context<MultisoulApp>,
) -> Option<gpui::Stateful<Div>> {
    let (title, help) = match (&call.error, &call.tools_error) {
        (Some(_), _) => (
            ui::i18n::text("voice.live_call_interrupted"),
            ui::i18n::format(
                "voice.live_call_interrupted_help",
                &[("shortcut", shortcut.to_string())],
            ),
        ),
        (None, Some(error)) => (
            ui::i18n::text("voice.live_tools_unavailable"),
            error.clone(),
        ),
        (None, None) => return None,
    };
    let color = theme.accent_warm;
    let expanded = state.gpt_live_diagnostics_expanded();
    Some(
        div()
            .id("settings-live-alert")
            .debug_selector(|| "settings-live-alert".into())
            .flex()
            .items_start()
            .gap(px(10.0))
            .mb(px(14.0))
            .px(px(14.0))
            .py(px(12.0))
            .rounded(px(12.0))
            .bg(rgba(with_alpha(color, 0x1c)))
            .child(
                div()
                    .pt(px(2.0))
                    .child(icon("icons/triangle-alert.svg", color, 18.0)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .settings_font(TextRole::RowTitle)
                            .text_color(rgba(color))
                            .child(title),
                    )
                    .child(helper(theme, help)),
            )
            .when(!expanded, |alert| {
                alert.child(text_button(
                    theme,
                    "settings-live-alert-details",
                    ui::i18n::text("voice.live_view_details"),
                    None,
                    cx,
                    |app, _, cx| {
                        app.voice_settings.toggle_gpt_live_diagnostics();
                        cx.notify();
                    },
                ))
            }),
    )
}

/// In-call only: the idle page stays pure configuration, but a live call keeps mute and stop-speaking reachable.
fn call_controls(
    theme: Theme,
    call: &LiveCallView,
    cx: &mut Context<MultisoulApp>,
) -> gpui::Stateful<Div> {
    div()
        .id("settings-live-call-status")
        .debug_selector(|| "settings-live-call-status".into())
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
        .mb(px(14.0))
        .px(px(14.0))
        .py(px(10.0))
        .rounded(px(12.0))
        .bg(rgba(with_alpha(theme.accent_blue, 0x14)))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .settings_text(TextRole::Body, theme)
                .child(call.status.clone()),
        )
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
        ))
}

fn section_title(theme: Theme) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .mt(px(26.0))
        .mb(px(11.0))
        .child(
            div()
                .settings_text(TextRole::RowTitle, theme)
                .child(ui::i18n::text("voice.live_preferences")),
        )
        .child(
            div()
                .settings_text(TextRole::Caption, theme)
                .child(ui::i18n::text("voice.live_auto_save")),
        )
}

fn preferences(
    theme: Theme,
    state: &VoiceSettingsState,
    shortcut: &str,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let mode = state.values().gpt_live_interaction_mode();
    let modes = segmented(
        theme,
        [
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
        ],
        mode,
        cx,
        |app, value, cx| app.select_gpt_live_interaction_mode(value, cx),
    );
    let selected = state.values().gpt_live_profile().voice();
    let voices = div().flex().gap(px(8.0)).children(
        [
            ("marin", "settings-live-voice-marin", "Marin"),
            ("cedar", "settings-live-voice-cedar", "Cedar"),
        ]
        .map(|(id, selector, label)| voice_option(theme, id, selector, label, selected == id, cx)),
    );
    card(theme)
        .child(
            setting_row(
                theme,
                ui::i18n::text("voice.live_interaction"),
                ui::i18n::format(
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
        .child(divider(theme))
        .child(
            setting_row(
                theme,
                ui::i18n::text("voice.live_reply_voice"),
                ui::i18n::text("voice.live_voice_next_call"),
            )
            .child(voices),
        )
        .child(divider(theme))
        .child(device::row(theme, state, cx))
}

fn footer_status(state: &VoiceSettingsState) -> Option<&'static str> {
    if state.settings_persistence_pending() || state.credential_persistence_pending() {
        Some("voice.saving")
    } else if state.status().is_some_and(|(_, error)| error)
        || state.has_gpt_live_credential_retry()
    {
        None
    } else {
        Some("voice.live_all_saved")
    }
}

fn footer(theme: Theme, state: &VoiceSettingsState) -> Div {
    let status = footer_status(state);
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(6.0))
        .pt(px(10.0))
        .children(
            status
                .filter(|status| *status == "voice.live_all_saved")
                .map(|_| icon("icons/check.svg", theme.text_tertiary, 13.0)),
        )
        .children(status.map(|status| {
            div()
                .id("settings-live-save-status")
                .debug_selector(|| "settings-live-save-status".into())
                .settings_text(TextRole::Caption, theme)
                .child(ui::i18n::text(status))
        }))
        .child(
            div()
                .ml_auto()
                .settings_text(TextRole::Caption, theme)
                .child(ui::i18n::text("voice.live_follows_system_input")),
        )
}

fn card(theme: Theme) -> Div {
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .rounded(px(16.0))
        .border_1()
        .border_color(rgba(line(theme)))
}

fn divider(theme: Theme) -> Div {
    div().w_full().h(px(1.0)).bg(rgba(line(theme)))
}

fn line(theme: Theme) -> u32 {
    with_alpha(theme.text_primary, 0x18)
}

fn setting_row(theme: Theme, title: impl Into<SharedString>, help: impl Into<SharedString>) -> Div {
    div()
        .w_full()
        .flex()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap(px(14.0))
        .p(px(20.0))
        .child(
            div()
                .flex_1()
                .min_w(px(180.0))
                .flex()
                .flex_col()
                .gap(px(5.0))
                .child(
                    div()
                        .settings_text(TextRole::RowTitle, theme)
                        .child(title.into()),
                )
                .child(helper(theme, help)),
        )
}

fn helper(theme: Theme, text: impl Into<SharedString>) -> Div {
    div()
        .settings_text(TextRole::Helper, theme)
        .child(text.into())
}

fn icon(path: &'static str, color: u32, size: f32) -> gpui::Svg {
    svg()
        .path(SharedString::from(path))
        .flex_none()
        .size(px(size))
        .text_color(rgba(color))
}

/// Low-emphasis Material text button; the label carries the accent colour.
fn text_button(
    theme: Theme,
    selector: &'static str,
    label: impl Into<SharedString>,
    trailing_icon: Option<&'static str>,
    cx: &Context<MultisoulApp>,
    action: impl Fn(&mut MultisoulApp, &mut Window, &mut Context<MultisoulApp>) + 'static,
) -> gpui::Stateful<Div> {
    let label = label.into();
    div()
        .id(selector)
        .debug_selector(move || selector.into())
        .flex_none()
        .child(
            Button::new(selector)
                .ghost()
                .compact()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.0))
                        .settings_font(TextRole::Control)
                        .text_color(rgba(theme.accent_blue))
                        .child(label)
                        .children(trailing_icon.map(|path| icon(path, theme.accent_blue, 14.0))),
                )
                .on_click(cx.listener(move |app, _, window, cx| action(app, window, cx))),
        )
}

/// Tonal bordered action used for secondary commands (remove, retry, grant, reconcile).
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
                        .settings_font(TextRole::Control)
                        .when(disabled, |label| {
                            label.text_color(rgba(theme.text_secondary))
                        })
                        .child(label),
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
