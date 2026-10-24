//! 设置 → Norma 大脑：按原型的表单排布，所有选项都落到 `~/.norma/brain/config.json`。

mod widgets;

use crate::app::{BrainSettings, BrainSettingsView, MultisoulApp, SlowReminder, SummaryEvery};
use crate::settings::typography::{control_label, SettingsText, TextRole};
use brain::{BackendKind, Strategy};
use gpui::{div, prelude::*, px, rgba, Context, Div};
use ui::component::button::Button;
use ui::component::checkbox::Checkbox;
use ui::component::switch::Switch;
use ui::component::{Disableable, Sizable as _, Size};
use ui::Theme;
use widgets::{backend_card, chip, form_row, note, segment, wrap_row};

const BACKENDS: [(&str, &str, BackendKind, &str); 3] = [
    (
        "settings-brain-backend-claude",
        "Claude Code",
        BackendKind::ClaudeCode,
        "claude-agent-acp",
    ),
    (
        "settings-brain-backend-codex",
        "Codex",
        BackendKind::Codex,
        "codex-acp",
    ),
    (
        "settings-brain-backend-opencode",
        "OpenCode",
        BackendKind::OpenCode,
        "opencode acp",
    ),
];

pub(super) fn settings(
    theme: Theme,
    view: &BrainSettingsView,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let settings = &view.settings;
    let claude = view.chosen_backend() == Some(BackendKind::ClaudeCode);
    let mut form = div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(18.0))
        .px(px(18.0))
        .py(px(18.0))
        .rounded(px(16.0))
        .border_1()
        .border_color(rgba(theme.node_stroke))
        .bg(rgba(theme.panel_high))
        .child(enable_row(theme, view, cx))
        .child(form_row(
            theme,
            ui::i18n::text("settings.brain_backend"),
            backend_control(theme, view, cx),
        ))
        .child(form_row(
            theme,
            ui::i18n::text("settings.brain_strategy"),
            strategy_control(theme, settings, cx),
        ));
    form = form
        .child(form_row(
            theme,
            ui::i18n::text("settings.brain_main_model"),
            model_control(theme, claude, settings.main_model.clone(), true, cx),
        ))
        .child(form_row(
            theme,
            ui::i18n::text("settings.brain_approval_model"),
            model_control(theme, claude, settings.approval_model.clone(), false, cx),
        ))
        .child(form_row(
            theme,
            ui::i18n::text("settings.brain_summary"),
            summary_control(settings, cx),
        ))
        .child(form_row(
            theme,
            ui::i18n::text("settings.brain_autonomy"),
            autonomy_control(settings, cx),
        ))
        .child(form_row(
            theme,
            ui::i18n::text("settings.brain_always_ask"),
            always_ask(theme),
        ))
        .child(form_row(
            theme,
            ui::i18n::text("settings.brain_slow"),
            slow_control(settings, cx),
        ))
        .child(form_row(
            theme,
            ui::i18n::text("settings.brain_parallel"),
            parallel_control(settings, cx),
        ))
        .child(form_row(
            theme,
            ui::i18n::text("settings.brain_scope"),
            div()
                .settings_text(TextRole::Helper, theme)
                .max_w(px(560.0))
                .child(ui::i18n::text("settings.brain_scope_note")),
        ));
    div().w_full().flex().flex_col().gap(px(14.0)).child(form)
}

fn enable_row(theme: Theme, view: &BrainSettingsView, cx: &mut Context<MultisoulApp>) -> Div {
    let app = cx.weak_entity();
    let status = if view.running {
        ui::i18n::text("settings.brain_running")
    } else {
        ui::i18n::text("settings.brain_stopped")
    };
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(20.0))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(5.0))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(9.0))
                        .child(
                            div()
                                .settings_text(TextRole::RowTitle, theme)
                                .child(ui::i18n::text("settings.brain_enable")),
                        )
                        .child(
                            div()
                                .px(px(8.0))
                                .py(px(3.0))
                                .rounded(px(9.0))
                                .bg(rgba(theme.panel))
                                .settings_text(TextRole::Badge, theme)
                                .child(status),
                        ),
                )
                .child(
                    div()
                        .max_w(px(570.0))
                        .settings_text(TextRole::Helper, theme)
                        .child(ui::i18n::text("settings.brain_enable_hint")),
                ),
        )
        .child(
            div()
                .w(px(36.0))
                .h(px(20.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Switch::new("settings-brain-enabled")
                        .checked(view.settings.enabled)
                        .with_size(Size::Medium)
                        .on_click(move |checked, _window, cx| {
                            let enabled = *checked;
                            let _ = app.update(cx, |app, cx| {
                                app.update_brain_settings(move |s| s.enabled = enabled, cx)
                            });
                        }),
                ),
        )
}

fn backend_control(theme: Theme, view: &BrainSettingsView, cx: &mut Context<MultisoulApp>) -> Div {
    let mut cards = wrap_row();
    for (id, name, kind, program) in BACKENDS {
        let installed = view.installed.contains(&kind);
        let status = if installed {
            format!(
                "{} · {program}",
                ui::i18n::text("settings.brain_backend_installed")
            )
        } else {
            ui::i18n::text("settings.brain_backend_no_adapter").to_string()
        };
        cards = cards.child(backend_card(
            theme,
            id,
            name,
            status,
            installed && view.chosen_backend() == Some(kind),
            installed,
            cx,
            move |app, cx| app.update_brain_settings(move |s| s.backend = Some(kind), cx),
        ));
    }
    let mut column = div().flex().flex_col().gap(px(10.0)).child(cards);
    if !view.installed.contains(&BackendKind::ClaudeCode) {
        let label = if view.installing {
            ui::i18n::text("settings.brain_installing")
        } else {
            ui::i18n::text("settings.brain_install")
        };
        column = column.child(
            div().flex().flex_row().child(
                Button::new("settings-brain-install")
                    .child(control_label(label))
                    .outline()
                    .disabled(view.installing)
                    .on_click(cx.listener(|app, _, _, cx| app.install_brain_adapter(cx))),
            ),
        );
    }
    if let Some(reason) = &view.install_error {
        column = column.child(div().settings_text(TextRole::Helper, theme).child(
            ui::i18n::format(
                "settings.brain_install_failed",
                &[("reason", reason.clone())],
            ),
        ));
    }
    column
}

fn strategy_control(theme: Theme, settings: &BrainSettings, cx: &mut Context<MultisoulApp>) -> Div {
    let current = settings.strategy;
    let option = |id, strategy: Strategy, label, cx: &mut Context<MultisoulApp>| {
        segment(id, label, current == strategy, false, cx, move |app, cx| {
            app.update_brain_settings(move |s| s.strategy = strategy, cx)
        })
    };
    let note_key = match current {
        Strategy::Speed => "settings.brain_strategy_note_speed",
        Strategy::Auto => "settings.brain_strategy_note_auto",
        Strategy::Quality => "settings.brain_strategy_note_quality",
    };
    div()
        .child(
            wrap_row()
                .child(option(
                    "settings-brain-strategy-speed",
                    Strategy::Speed,
                    ui::i18n::text("settings.brain_strategy_speed"),
                    cx,
                ))
                .child(option(
                    "settings-brain-strategy-auto",
                    Strategy::Auto,
                    ui::i18n::text("settings.brain_strategy_auto"),
                    cx,
                ))
                .child(option(
                    "settings-brain-strategy-quality",
                    Strategy::Quality,
                    ui::i18n::text("settings.brain_strategy_quality"),
                    cx,
                )),
        )
        .child(note(theme, ui::i18n::text(note_key)))
}

/// 模型只对 Claude Code 可选（别名 opus、sonnet、haiku）；其他后端跟随各自 CLI。
fn model_control(
    theme: Theme,
    claude: bool,
    current: Option<String>,
    main: bool,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    if !claude {
        return div()
            .pt(px(6.0))
            .settings_text(TextRole::Helper, theme)
            .child(ui::i18n::text("settings.brain_model_cli_default"));
    }
    let choices: [(&'static str, Option<&'static str>, &'static str); 4] = [
        (
            "follow",
            None,
            ui::i18n::text("settings.brain_model_follow_strategy"),
        ),
        ("opus", Some("opus"), "Opus"),
        ("sonnet", Some("sonnet"), "Sonnet"),
        ("haiku", Some("haiku"), "Haiku"),
    ];
    let mut row = wrap_row();
    for (key, alias, label) in choices {
        let id: &'static str = match (main, key) {
            (true, "follow") => "settings-brain-main-model-follow",
            (true, "opus") => "settings-brain-main-model-opus",
            (true, "sonnet") => "settings-brain-main-model-sonnet",
            (true, _) => "settings-brain-main-model-haiku",
            (false, "follow") => "settings-brain-approval-model-follow",
            (false, "opus") => "settings-brain-approval-model-opus",
            (false, "sonnet") => "settings-brain-approval-model-sonnet",
            (false, _) => "settings-brain-approval-model-haiku",
        };
        let selected = current.as_deref() == alias;
        row = row.child(segment(id, label, selected, false, cx, move |app, cx| {
            let value = alias.map(str::to_string);
            app.update_brain_settings(
                move |s| {
                    if main {
                        s.main_model = value;
                    } else {
                        s.approval_model = value;
                    }
                },
                cx,
            )
        }));
    }
    row
}

fn summary_control(settings: &BrainSettings, cx: &mut Context<MultisoulApp>) -> Div {
    let current = settings.summary;
    let option = |id, value: SummaryEvery, label, cx: &mut Context<MultisoulApp>| {
        segment(id, label, current == value, false, cx, move |app, cx| {
            app.update_brain_settings(move |s| s.summary = value, cx)
        })
    };
    wrap_row()
        .child(option(
            "settings-brain-summary-5m",
            SummaryEvery::FiveMinutes,
            ui::i18n::text("settings.brain_summary_5m"),
            cx,
        ))
        .child(option(
            "settings-brain-summary-10m",
            SummaryEvery::TenMinutes,
            ui::i18n::text("settings.brain_summary_10m"),
            cx,
        ))
        .child(option(
            "settings-brain-summary-off",
            SummaryEvery::Off,
            ui::i18n::text("settings.brain_summary_off"),
            cx,
        ))
}

fn autonomy_control(settings: &BrainSettings, cx: &mut Context<MultisoulApp>) -> Div {
    let a = settings.autonomy;
    let app = cx.weak_entity();
    let check = move |id: &'static str,
                      label: &'static str,
                      checked: bool,
                      apply: fn(&mut BrainSettings, bool)| {
        let app = app.clone();
        Checkbox::new(id)
            .label(label)
            .checked(checked)
            .on_click(move |value, _window, cx| {
                let value = *value;
                let _ = app.update(cx, |app, cx| {
                    app.update_brain_settings(move |s| apply(s, value), cx)
                });
            })
    };
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(check(
            "settings-brain-autonomy-readonly",
            ui::i18n::text("settings.brain_autonomy_readonly"),
            a.readonly_commands,
            |s, v| s.autonomy.readonly_commands = v,
        ))
        .child(check(
            "settings-brain-autonomy-writes",
            ui::i18n::text("settings.brain_autonomy_writes"),
            a.project_writes,
            |s, v| s.autonomy.project_writes = v,
        ))
        .child(check(
            "settings-brain-autonomy-assign",
            ui::i18n::text("settings.brain_autonomy_assign"),
            a.assign_followups,
            |s, v| s.autonomy.assign_followups = v,
        ))
        .child(
            Checkbox::new("settings-brain-autonomy-stop")
                .label(ui::i18n::text("settings.brain_autonomy_stop"))
                .checked(false)
                .disabled(true),
        )
}

fn always_ask(theme: Theme) -> Div {
    wrap_row()
        .child(chip(theme, ui::i18n::text("settings.brain_ask_push")))
        .child(chip(theme, ui::i18n::text("settings.brain_ask_delete")))
        .child(chip(theme, ui::i18n::text("settings.brain_ask_outside")))
        .child(chip(theme, ui::i18n::text("settings.brain_ask_deploy")))
        .child(chip(theme, ui::i18n::text("settings.brain_ask_secrets")))
        .child(chip(theme, ui::i18n::text("settings.brain_ask_privilege")))
}

fn slow_control(settings: &BrainSettings, cx: &mut Context<MultisoulApp>) -> Div {
    let current = settings.slow_reminder;
    let option = |id, value: SlowReminder, label, cx: &mut Context<MultisoulApp>| {
        segment(id, label, current == value, false, cx, move |app, cx| {
            app.update_brain_settings(move |s| s.slow_reminder = value, cx)
        })
    };
    wrap_row()
        .child(option(
            "settings-brain-slow-overview",
            SlowReminder::OverviewOnly,
            ui::i18n::text("settings.brain_slow_overview"),
            cx,
        ))
        .child(option(
            "settings-brain-slow-voice",
            SlowReminder::VoiceAfter60s,
            ui::i18n::text("settings.brain_slow_voice"),
            cx,
        ))
        .child(option(
            "settings-brain-slow-off",
            SlowReminder::Off,
            ui::i18n::text("settings.brain_slow_off"),
            cx,
        ))
}

fn parallel_control(settings: &BrainSettings, cx: &mut Context<MultisoulApp>) -> Div {
    let current = settings.parallel_approvals;
    wrap_row()
        .child(segment(
            "settings-brain-parallel-on",
            ui::i18n::text("settings.brain_parallel_on"),
            current,
            false,
            cx,
            |app, cx| app.update_brain_settings(|s| s.parallel_approvals = true, cx),
        ))
        .child(segment(
            "settings-brain-parallel-off",
            ui::i18n::text("settings.brain_parallel_off"),
            !current,
            false,
            cx,
            |app, cx| app.update_brain_settings(|s| s.parallel_approvals = false, cx),
        ))
}
