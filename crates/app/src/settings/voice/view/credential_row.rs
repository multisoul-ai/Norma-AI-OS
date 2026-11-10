//! Shared API-key row: save, clear, and a saved/missing status.
//! The field stays masked. A saved key is never copied back into the editor or the logs.
//!
//! Apple: the status sits with the label, the field is the direct control, and there is no motion.
//! Material: tonal save/clear buttons keep hover, pressed, focus, and disabled on the component button.
//! Reduced motion does not apply; these controls only change color.

use super::VoiceSettingsInputs;
use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use crate::settings::voice::VoiceSettingsField;
use gpui::{div, prelude::*, px, rgba, Context, Div, KeyDownEvent, MouseButton, Window};
use std::rc::Rc;
use ui::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use ui::component::Disableable;
use ui::Theme;

pub(in crate::settings::voice::view) struct CredentialRowSpec {
    pub label: &'static str,
    pub hint: &'static str,
    pub field: VoiceSettingsField,
    pub configured: bool,
    pub pending: bool,
    pub status_id: &'static str,
    pub save_id: &'static str,
    pub clear_id: &'static str,
}

pub(in crate::settings::voice::view) fn credential_row(
    theme: Theme,
    spec: CredentialRowSpec,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
    on_clear: impl Fn(&mut MultisoulApp, &mut Window, &mut Context<MultisoulApp>) + 'static,
) -> Div {
    let field = spec.field;
    let pending = spec.pending;
    let status_id = spec.status_id;
    let (status_key, status_color) = if pending {
        ("voice.saving", theme.text_secondary)
    } else if spec.configured {
        ("voice.live_key_saved", theme.accent_green)
    } else {
        ("voice.not_configured", theme.accent_warm)
    };
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .py(px(8.0))
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .settings_text(TextRole::RowTitle, theme)
                        .child(ui::i18n::text(spec.label)),
                )
                .child(
                    div()
                        .id(status_id)
                        .debug_selector(move || status_id.to_string())
                        .flex_none()
                        .settings_font(TextRole::Caption)
                        .text_color(rgba(status_color))
                        .child(ui::i18n::text(status_key)),
                ),
        )
        .child(
            div()
                .w_full()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(8.0))
                .child(div().flex_1().min_w(px(160.0)).child(inputs.input(field)))
                .when(spec.configured, |row| {
                    row.child(credential_action(
                        theme,
                        spec.clear_id,
                        ui::i18n::text("voice.clear"),
                        theme.destructive,
                        pending,
                        cx,
                        move |app, window, cx| {
                            app.cancel_voice_text_input(field, cx);
                            on_clear(app, window, cx);
                            app.voice_settings_inputs.sync_field_from_state(
                                field,
                                &app.voice_settings,
                                cx,
                            );
                        },
                    ))
                })
                .child(credential_action(
                    theme,
                    spec.save_id,
                    ui::i18n::text("common.save"),
                    theme.accent_blue,
                    pending,
                    cx,
                    move |app, _, cx| {
                        let value = app.voice_settings_inputs.draft_text(field, cx);
                        if app.commit_voice_text_input(field, value, cx).is_some() {
                            app.voice_settings_inputs.sync_field_from_state(
                                field,
                                &app.voice_settings,
                                cx,
                            );
                        }
                    },
                )),
        )
        .child(
            div()
                .settings_text(TextRole::Helper, theme)
                .child(ui::i18n::text(spec.hint)),
        )
}

fn credential_action(
    theme: Theme,
    selector: &'static str,
    label: &'static str,
    accent: u32,
    disabled: bool,
    cx: &Context<MultisoulApp>,
    action: impl Fn(&mut MultisoulApp, &mut Window, &mut Context<MultisoulApp>) + 'static,
) -> gpui::Stateful<Div> {
    let action = Rc::new(action);
    let on_click = Rc::clone(&action);
    let on_key = action;
    let host = cx.entity().downgrade();
    let variant = ButtonCustomVariant::new(cx)
        .color(
            rgba(super::with_alpha(
                accent,
                if disabled { 0x08 } else { 0x14 },
            ))
            .into(),
        )
        .foreground(
            rgba(if disabled {
                theme.text_secondary
            } else {
                accent
            })
            .into(),
        )
        .border(
            rgba(super::with_alpha(
                accent,
                if disabled { 0x28 } else { 0x70 },
            ))
            .into(),
        )
        .hover(rgba(super::with_alpha(accent, 0x30)).into())
        .active(rgba(super::with_alpha(accent, 0x48)).into());
    div()
        .id(selector)
        .debug_selector(move || selector.to_string())
        .flex_none()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            Button::new(selector)
                .child(
                    div()
                        .settings_font(TextRole::Control)
                        .text_color(rgba(if disabled {
                            theme.text_secondary
                        } else {
                            accent
                        }))
                        .child(label),
                )
                .compact()
                .custom(variant)
                .disabled(disabled)
                .on_click(cx.listener(move |app, _, window, cx| {
                    if !disabled {
                        on_click(app, window, cx);
                    }
                }))
                .on_key_down(move |event: &KeyDownEvent, window, cx| {
                    if disabled || !matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        return;
                    }
                    if let Some(host) = host.upgrade() {
                        host.update(cx, |app, cx| on_key(app, window, cx));
                    }
                    cx.stop_propagation();
                }),
        )
}
