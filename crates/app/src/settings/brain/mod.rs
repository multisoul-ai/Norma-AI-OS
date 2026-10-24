//! 设置 → 幕僚：顶部状态卡加五个分组（汇报、下一步与自主权、始终先问你、模型、高级），
//! 按 Material 3 的列表排布。所有选项都落到 `~/.norma/brain/config.json`。

mod backend_models;
mod models;
mod widgets;

use crate::app::{BrainSettings, BrainSettingsView, MultisoulApp, SlowReminder, SummaryEvery};
use crate::settings::typography::{SettingsText, TextRole};
use gpui::{div, prelude::*, px, rgba, AnyElement, Context, Div};
use models::{model_name, model_rows, BACKENDS};
use ui::component::Disableable;
use ui::icons::with_alpha;
use ui::Theme;
use widgets::{chip, note, row, section, segmented, switch, Segment};

pub(super) fn t(key: &'static str) -> &'static str {
    ui::i18n::text(key)
}

/// 一个分段选项，点击后改一项设置。
pub(super) fn seg(
    id: &'static str,
    label: &'static str,
    selected: bool,
    apply: impl Fn(&mut BrainSettings) + 'static,
) -> Segment {
    (
        id,
        label,
        selected,
        Box::new(move |app, cx| app.update_brain_settings(|s| apply(s), cx)),
    )
}

pub(super) fn settings(
    theme: Theme,
    view: &BrainSettingsView,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let s = &view.settings;
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(28.0))
        .child(hero(theme, view, cx))
        .child(section(
            theme,
            t("settings.brain_section_report"),
            report_rows(theme, s, cx),
        ))
        .child(section(
            theme,
            t("settings.brain_section_next"),
            next_rows(theme, s, cx),
        ))
        .child(section(
            theme,
            t("settings.brain_always_ask"),
            ask_rows(theme),
        ))
        .child(section(
            theme,
            t("settings.brain_section_models"),
            model_rows(theme, view, cx),
        ))
        .child(section(
            theme,
            t("settings.brain_section_advanced"),
            advanced_rows(theme, s, cx),
        ))
}

/// 状态卡：只回答“开没开、在不在跑、用的什么”，总开关也在这里。
fn hero(theme: Theme, view: &BrainSettingsView, cx: &mut Context<MultisoulApp>) -> Div {
    let on = view.settings.enabled && view.running;
    let (dot, status) = if on {
        (theme.success, t("settings.brain_running"))
    } else {
        (theme.text_tertiary, t("settings.brain_stopped"))
    };
    let backend = BACKENDS
        .iter()
        .find(|(_, _, kind, _)| view.chosen_backend() == Some(*kind))
        .map_or("—", |(_, name, _, _)| *name);
    let meta = format!(
        "{backend} · {} · {}",
        model_name(view),
        t("settings.brain_enable_hint")
    );
    div()
        .w_full()
        .px(px(20.0))
        .py(px(16.0))
        .rounded(px(16.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.text_primary, 0x1f)))
        .bg(rgba(theme.panel_high))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(16.0))
        .child(
            div()
                .size(px(10.0))
                .flex_none()
                .rounded_full()
                .bg(rgba(dot))
                .border_4()
                .border_color(rgba(with_alpha(dot, 0x33))),
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
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .settings_text(TextRole::Headline, theme)
                                .child(t("settings.brain_enable")),
                        )
                        .child(
                            div()
                                .px(px(8.0))
                                .py(px(2.0))
                                .rounded(px(8.0))
                                .bg(rgba(with_alpha(dot, 0x26)))
                                .settings_text(TextRole::Badge, theme)
                                .text_color(rgba(dot))
                                .child(status),
                        ),
                )
                .child(div().settings_text(TextRole::Helper, theme).child(meta)),
        )
        .child(switch(
            "settings-brain-enabled",
            view.settings.enabled,
            cx,
            |app, v, cx| app.update_brain_settings(move |s| s.enabled = v, cx),
        ))
}

fn report_rows(theme: Theme, s: &BrainSettings, cx: &mut Context<MultisoulApp>) -> Vec<AnyElement> {
    let summary = s.summary;
    let slow = s.slow_reminder;
    vec![
        row(
            theme,
            t("settings.brain_voice"),
            Some(t("settings.brain_voice_hint").into()),
            switch("settings-brain-voice", s.speak, cx, |app, v, cx| {
                app.set_brain_speaking(v, cx)
            }),
        )
        .into_any_element(),
        row(
            theme,
            t("settings.brain_summary"),
            Some(t("settings.brain_summary_hint").into()),
            segmented(
                "settings-brain-summary",
                vec![
                    seg(
                        "settings-brain-summary-5m",
                        t("settings.brain_summary_5m"),
                        summary == SummaryEvery::FiveMinutes,
                        |s| s.summary = SummaryEvery::FiveMinutes,
                    ),
                    seg(
                        "settings-brain-summary-10m",
                        t("settings.brain_summary_10m"),
                        summary == SummaryEvery::TenMinutes,
                        |s| s.summary = SummaryEvery::TenMinutes,
                    ),
                    seg(
                        "settings-brain-summary-off",
                        t("settings.brain_summary_off"),
                        summary == SummaryEvery::Off,
                        |s| s.summary = SummaryEvery::Off,
                    ),
                ],
                cx,
            ),
        )
        .into_any_element(),
        row(
            theme,
            t("settings.brain_slow"),
            Some(t("settings.brain_slow_hint").into()),
            segmented(
                "settings-brain-slow",
                vec![
                    seg(
                        "settings-brain-slow-overview",
                        t("settings.brain_slow_overview"),
                        slow == SlowReminder::OverviewOnly,
                        |s| s.slow_reminder = SlowReminder::OverviewOnly,
                    ),
                    seg(
                        "settings-brain-slow-voice",
                        t("settings.brain_slow_voice"),
                        slow == SlowReminder::VoiceAfter60s,
                        |s| s.slow_reminder = SlowReminder::VoiceAfter60s,
                    ),
                    seg(
                        "settings-brain-slow-off",
                        t("settings.brain_slow_off"),
                        slow == SlowReminder::Off,
                        |s| s.slow_reminder = SlowReminder::Off,
                    ),
                ],
                cx,
            ),
        )
        .into_any_element(),
    ]
}

fn next_rows(theme: Theme, s: &BrainSettings, cx: &mut Context<MultisoulApp>) -> Vec<AnyElement> {
    let a = s.autonomy;
    vec![
        row(
            theme,
            t("settings.brain_followups"),
            Some(t("settings.brain_followups_hint").into()),
            segmented(
                "settings-brain-followups",
                vec![
                    seg(
                        "settings-brain-followups-ask",
                        t("settings.brain_followups_ask"),
                        !a.assign_followups,
                        |s| s.autonomy.assign_followups = false,
                    ),
                    seg(
                        "settings-brain-followups-auto",
                        t("settings.brain_followups_auto"),
                        a.assign_followups,
                        |s| s.autonomy.assign_followups = true,
                    ),
                ],
                cx,
            ),
        )
        .into_any_element(),
        row(
            theme,
            t("settings.brain_autonomy_readonly"),
            Some(t("settings.brain_autonomy_readonly_hint").into()),
            switch(
                "settings-brain-autonomy-readonly",
                a.readonly_commands,
                cx,
                |app, v, cx| {
                    app.update_brain_settings(move |s| s.autonomy.readonly_commands = v, cx)
                },
            ),
        )
        .into_any_element(),
        row(
            theme,
            t("settings.brain_autonomy_writes"),
            Some(t("settings.brain_autonomy_writes_hint").into()),
            switch(
                "settings-brain-autonomy-writes",
                a.project_writes,
                cx,
                |app, v, cx| app.update_brain_settings(move |s| s.autonomy.project_writes = v, cx),
            ),
        )
        .into_any_element(),
        row(
            theme,
            t("settings.brain_autonomy_stop"),
            Some(t("settings.brain_coming_soon").into()),
            ui::component::switch::Switch::new("settings-brain-autonomy-stop")
                .checked(false)
                .disabled(true),
        )
        .opacity(0.55)
        .into_any_element(),
    ]
}

fn ask_rows(theme: Theme) -> Vec<AnyElement> {
    let chips = [
        "settings.brain_ask_push",
        "settings.brain_ask_delete",
        "settings.brain_ask_outside",
        "settings.brain_ask_deploy",
        "settings.brain_ask_secrets",
        "settings.brain_ask_privilege",
    ];
    vec![
        div()
            .w_full()
            .px(px(20.0))
            .pt(px(14.0))
            .pb(px(16.0))
            .flex()
            .flex_wrap()
            .gap(px(8.0))
            .children(chips.map(|key| chip(theme, t(key))))
            .into_any_element(),
        note(theme, t("settings.brain_always_ask_note")).into_any_element(),
    ]
}

fn advanced_rows(
    theme: Theme,
    s: &BrainSettings,
    cx: &mut Context<MultisoulApp>,
) -> Vec<AnyElement> {
    vec![
        row(
            theme,
            t("settings.brain_parallel"),
            Some(t("settings.brain_parallel_hint").into()),
            switch(
                "settings-brain-parallel",
                s.parallel_approvals,
                cx,
                |app, v, cx| app.update_brain_settings(move |s| s.parallel_approvals = v, cx),
            ),
        )
        .into_any_element(),
        note(theme, t("settings.brain_scope_note")).into_any_element(),
    ]
}
