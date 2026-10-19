use super::*;

pub(super) fn section(
    theme: Theme,
    state: &VoiceSettingsState,
    call: &LiveCallView,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let expanded = state.gpt_live_diagnostics_expanded();
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(live_action(
            theme,
            "settings-live-diagnostics-toggle",
            ui::i18n::text(if expanded {
                "voice.live_diagnostics_hide"
            } else {
                "voice.live_diagnostics_show"
            }),
            false,
            false,
            cx,
            |app, _, cx| {
                app.voice_settings.toggle_gpt_live_diagnostics();
                cx.notify();
            },
        ))
        .when(expanded, |group| {
            group.child(
                super::section(theme)
                    .child(helper(theme, ui::i18n::text("voice.live_diagnostics_hint")))
                    .child(live_action(
                        theme,
                        "settings-live-reconcile",
                        ui::i18n::text("voice.live_reconcile_refresh"),
                        false,
                        call.reconciling,
                        cx,
                        |app, _, cx| app.request_gpt_live_reconciliation(cx),
                    ))
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
                    }),
            )
        })
}
