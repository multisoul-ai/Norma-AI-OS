use super::*;

/// Collapsed by default: facts, the raw last error, and task reconciliation for troubleshooting.
pub(super) fn section(
    theme: Theme,
    state: &VoiceSettingsState,
    call: &LiveCallView,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let expanded = state.gpt_live_diagnostics_expanded();
    div()
        .w_full()
        .mt(px(24.0))
        .border_t_1()
        .border_color(rgba(line(theme)))
        .flex()
        .flex_col()
        .child(toggle(theme, expanded, cx))
        .when(expanded, |section| {
            section.child(content(theme, state, call, cx))
        })
}

/// Whole row toggles by pointer; the chevron is the focusable control for keyboard users.
fn toggle(theme: Theme, expanded: bool, cx: &mut Context<MultisoulApp>) -> gpui::Stateful<Div> {
    let flip = |app: &mut MultisoulApp, cx: &mut Context<MultisoulApp>| {
        app.voice_settings.toggle_gpt_live_diagnostics();
        cx.notify();
    };
    div()
        .id("settings-live-diagnostics-toggle")
        .debug_selector(|| "settings-live-diagnostics-toggle".into())
        .w_full()
        .min_h(px(52.0))
        .px(px(4.0))
        .flex()
        .items_center()
        .gap(px(9.0))
        .rounded(px(8.0))
        .cursor_pointer()
        .hover(|row| row.bg(rgba(with_alpha(theme.text_primary, 0x0d))))
        .active(|row| row.bg(rgba(with_alpha(theme.text_primary, 0x18))))
        .on_click(cx.listener(move |app, _, _, cx| flip(app, cx)))
        .child(icon("icons/settings-2.svg", theme.text_secondary, 17.0))
        .child(
            div()
                .settings_text(TextRole::Body, theme)
                .text_color(rgba(theme.text_secondary))
                .child(ui::i18n::text("voice.live_diagnostics_title")),
        )
        .child(
            div()
                .ml_auto()
                .settings_text(TextRole::Caption, theme)
                .child(ui::i18n::text("voice.live_diagnostics_note")),
        )
        .child(
            div()
                .id("settings-live-diagnostics-chevron")
                .debug_selector(|| "settings-live-diagnostics-chevron".into())
                .child(
                    Button::new("settings-live-diagnostics-chevron")
                        .ghost()
                        .compact()
                        .tooltip(ui::i18n::text("voice.live_diagnostics_title"))
                        .child(icon(
                            if expanded {
                                "icons/chevron-up.svg"
                            } else {
                                "icons/chevron-down.svg"
                            },
                            theme.text_secondary,
                            16.0,
                        ))
                        .on_click(cx.listener(move |app, event: &gpui::ClickEvent, _, cx| {
                            // Pointer clicks bubble to the row; only keyboard activation toggles here.
                            if event.is_keyboard() {
                                flip(app, cx);
                            }
                        })),
                ),
        )
}

fn content(
    theme: Theme,
    state: &VoiceSettingsState,
    call: &LiveCallView,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let device = state
        .input_devices()
        .iter()
        .find(|device| device.is_default)
        .map(|device| device.name.clone())
        .unwrap_or_else(|| ui::i18n::text("voice.live_diag_not_detected").to_string());
    let raw = call
        .error
        .clone()
        .or_else(|| call.tools_error.clone())
        .unwrap_or_else(|| ui::i18n::text("voice.live_no_errors").to_string());
    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .px(px(8.0))
        .pb(px(18.0))
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_x(px(24.0))
                .gap_y(px(10.0))
                .child(fact(
                    theme,
                    "voice.live_diag_service",
                    ui::i18n::text("voice.live_diag_service_value").to_string(),
                ))
                .child(fact(theme, "voice.live_diag_input", device))
                .child(fact(
                    theme,
                    "voice.live_diag_transport",
                    ui::i18n::text("voice.live_diag_transport_value").to_string(),
                ))
                .child(fact(
                    theme,
                    "voice.live_diag_buffer",
                    ui::i18n::text("voice.live_diag_buffer_value").to_string(),
                )),
        )
        .child(
            div()
                .id("settings-live-raw-error")
                .debug_selector(|| "settings-live-raw-error".into())
                .p(px(12.0))
                .rounded(px(8.0))
                .bg(rgba(with_alpha(theme.text_primary, 0x0a)))
                .settings_text(TextRole::Helper, theme)
                .font_family("Menlo")
                .text_color(rgba(theme.text_secondary))
                .child(raw),
        )
        .child(helper(theme, ui::i18n::text("voice.live_diagnostics_hint")))
        .child(div().flex().child(live_action(
            theme,
            "settings-live-reconcile",
            ui::i18n::text("voice.live_reconcile_refresh"),
            false,
            call.reconciling,
            cx,
            |app, _, cx| app.request_gpt_live_reconciliation(cx),
        )))
        .when_some(call.reconciliation.clone(), |group, text| {
            group.child(
                helper(theme, text)
                    .id("settings-live-reconciliation-report")
                    .debug_selector(|| "settings-live-reconciliation-report".into()),
            )
        })
        .when(call.usage_pending, |group| {
            group.child(helper(theme, ui::i18n::text("voice.live_usage_pending")))
        })
        .when_some(call.usage, |group, usage| {
            group.child(
                helper(
                    theme,
                    ui::i18n::format(
                        if usage.finalized {
                            "voice.live_usage_final"
                        } else {
                            "voice.live_usage_unconfirmed"
                        },
                        &[("seconds", format!("{:.2}", usage.seconds))],
                    ),
                )
                .id("settings-live-usage")
                .debug_selector(|| "settings-live-usage".into()),
            )
        })
        .child(
            div()
                .settings_text(TextRole::Caption, theme)
                .child(ui::i18n::text("voice.live_diag_privacy")),
        )
}

fn fact(theme: Theme, label: &'static str, value: String) -> Div {
    div()
        .min_w(px(220.0))
        .flex_1()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(10.0))
        .child(helper(theme, ui::i18n::text(label)))
        .child(div().settings_text(TextRole::Body, theme).child(value))
}
