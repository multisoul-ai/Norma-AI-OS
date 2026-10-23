//! 设置 → Norma 大脑：开关、后端、策略。

use crate::app::{BrainSettingsView, MultisoulApp};
use crate::settings::typography::{control_label, SettingsText, TextRole};
use brain::{BackendKind, Strategy};
use gpui::{div, prelude::*, px, rgba, Context, Div};
use ui::component::button::Button;
use ui::component::switch::Switch;
use ui::component::{Disableable, Selectable, Sizable as _, Size};
use ui::Theme;

fn card(theme: Theme) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .px(px(18.0))
        .py(px(16.0))
        .rounded(px(16.0))
        .border_1()
        .border_color(rgba(theme.node_stroke))
        .bg(rgba(theme.panel_high))
}

fn heading(theme: Theme, title: &'static str, hint: &'static str) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(div().settings_text(TextRole::RowTitle, theme).child(title))
        .child(
            div()
                .max_w(px(570.0))
                .settings_text(TextRole::Helper, theme)
                .child(hint),
        )
}

const BACKENDS: [(&str, BackendKind); 3] = [
    ("settings-brain-backend-claude", BackendKind::ClaudeCode),
    ("settings-brain-backend-codex", BackendKind::Codex),
    ("settings-brain-backend-opencode", BackendKind::OpenCode),
];

const STRATEGIES: [(&str, Strategy, &str); 3] = [
    (
        "settings-brain-strategy-speed",
        Strategy::Speed,
        "settings.brain_strategy_speed",
    ),
    (
        "settings-brain-strategy-auto",
        Strategy::Auto,
        "settings.brain_strategy_auto",
    ),
    (
        "settings-brain-strategy-quality",
        Strategy::Quality,
        "settings.brain_strategy_quality",
    ),
];

pub(super) fn settings(
    theme: Theme,
    view: &BrainSettingsView,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(14.0))
        .child(enable_card(theme, view, cx))
        .child(backend_card(theme, view, cx))
        .child(strategy_card(theme, view, cx))
}

fn enable_card(theme: Theme, view: &BrainSettingsView, cx: &mut Context<MultisoulApp>) -> Div {
    let app = cx.weak_entity();
    let status = if view.running {
        ui::i18n::text("settings.brain_running")
    } else {
        ui::i18n::text("settings.brain_stopped")
    };
    card(theme).child(
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
                            .checked(view.enabled)
                            .with_size(Size::Medium)
                            .on_click(move |checked, _window, cx| {
                                let _ =
                                    app.update(cx, |app, cx| app.set_brain_enabled(*checked, cx));
                            }),
                    ),
            ),
    )
}

fn backend_card(theme: Theme, view: &BrainSettingsView, cx: &mut Context<MultisoulApp>) -> Div {
    let mut row = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(8.0));
    for (id, kind) in BACKENDS {
        let installed = view.installed.contains(&kind);
        let selected = match view.backend {
            Some(chosen) => chosen == kind,
            None => view.installed.first() == Some(&kind),
        };
        let label = if installed {
            kind.label().to_string()
        } else {
            format!(
                "{} · {}",
                kind.label(),
                ui::i18n::text("settings.brain_not_installed")
            )
        };
        row = row.child(
            Button::new(id)
                .child(control_label(label))
                .selected(selected && installed)
                .disabled(!installed)
                .on_click(cx.listener(move |app, _, _, cx| app.set_brain_backend(Some(kind), cx))),
        );
    }
    let mut body = card(theme)
        .child(heading(
            theme,
            ui::i18n::text("settings.brain_backend"),
            ui::i18n::text("settings.brain_backend_hint"),
        ))
        .child(row);
    if !view.installed.contains(&BackendKind::ClaudeCode) {
        let label = if view.installing {
            ui::i18n::text("settings.brain_installing")
        } else {
            ui::i18n::text("settings.brain_install")
        };
        body = body.child(
            div().flex().flex_row().items_center().gap(px(10.0)).child(
                Button::new("settings-brain-install")
                    .child(control_label(label))
                    .outline()
                    .disabled(view.installing)
                    .on_click(cx.listener(|app, _, _, cx| app.install_brain_adapter(cx))),
            ),
        );
    }
    if let Some(reason) = &view.install_error {
        body = body.child(
            div()
                .settings_text(TextRole::Helper, theme)
                .child(ui::i18n::format(
                    "settings.brain_install_failed",
                    &[("reason", reason.clone())],
                )),
        );
    }
    body
}

fn strategy_card(theme: Theme, view: &BrainSettingsView, cx: &mut Context<MultisoulApp>) -> Div {
    let mut row = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(8.0));
    for (id, strategy, key) in STRATEGIES {
        row = row.child(
            Button::new(id)
                .child(control_label(ui::i18n::text(key)))
                .selected(view.strategy == strategy)
                .on_click(cx.listener(move |app, _, _, cx| app.set_brain_strategy(strategy, cx))),
        );
    }
    card(theme)
        .child(heading(
            theme,
            ui::i18n::text("settings.brain_strategy"),
            ui::i18n::text("settings.brain_strategy_hint"),
        ))
        .child(row)
}
