//! 幕僚设置的“模型”分组：后端、策略、主会话与审批会话的模型。

use super::widgets::{backend_row, row, segmented, text_button, BackendItem};
use super::{seg, t};
use crate::app::{BrainSettingsView, MultisoulApp};
use brain::{BackendKind, Strategy};
use gpui::{div, prelude::*, AnyElement, Context};
use ui::Theme;

pub(super) const BACKENDS: [(&str, &str, BackendKind, &str); 3] = [
    (
        "settings-brain-backend-claude",
        "Claude Code",
        BackendKind::ClaudeCode,
        "claude-agent-acp",
    ),
    (
        "settings-brain-backend-opencode",
        "OpenCode",
        BackendKind::OpenCode,
        "opencode acp",
    ),
    (
        "settings-brain-backend-codex",
        "Codex",
        BackendKind::Codex,
        "codex-acp",
    ),
];

/// 状态卡里显示的主会话模型。
pub(super) fn model_name(view: &BrainSettingsView) -> String {
    let Some(kind) = view.chosen_backend() else {
        return t("settings.brain_model_cli").to_string();
    };
    if kind != BackendKind::ClaudeCode {
        return view.settings.models_for(kind).main.map_or_else(
            || t("settings.brain_model_cli").to_string(),
            |id| super::backend_models::display_name(view.model_lists.get(&kind), &id),
        );
    }
    let alias = view.settings.main_model.clone().or_else(|| {
        brain::backend::models_for(BackendKind::ClaudeCode, view.settings.strategy).main
    });
    let alias = alias.unwrap_or_default();
    let mut chars = alias.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

pub(super) fn model_rows(
    theme: Theme,
    view: &BrainSettingsView,
    cx: &mut Context<MultisoulApp>,
) -> Vec<AnyElement> {
    let s = &view.settings;
    let mut rows = Vec::new();
    for (id, name, kind, program) in BACKENDS {
        let unavailable = brain::backend::unavailable_reason(kind).is_some();
        let installed = view.installed.contains(&kind) && !unavailable;
        let packaged = crate::app::brain_adapter_package(kind).is_some();
        let failed = view
            .install_error
            .as_ref()
            .filter(|(failed, _)| *failed == kind)
            .map(|(_, reason)| reason.clone());
        let status = if unavailable {
            t(if kind == BackendKind::Codex {
                "settings.brain_codex_unavailable"
            } else {
                "settings.brain_opencode_unavailable"
            })
            .to_string()
        } else if installed {
            format!("{} · {program}", t("settings.brain_backend_installed"))
        } else if let Some(reason) = failed {
            ui::i18n::format("settings.brain_install_failed", &[("reason", reason)])
        } else if packaged {
            t("settings.brain_backend_no_adapter").to_string()
        } else {
            t("settings.brain_opencode_missing").to_string()
        };
        let install = (packaged && !installed && !unavailable).then(|| {
            let busy = view.installing.is_some();
            let label = if view.installing == Some(kind) {
                t("settings.brain_installing")
            } else {
                t("settings.brain_install")
            };
            let id = if kind == BackendKind::Codex {
                "settings-brain-install-codex"
            } else {
                "settings-brain-install"
            };
            text_button(id, label, busy, cx, move |app, cx| {
                app.install_brain_adapter(kind, cx)
            })
        });
        let item = BackendItem {
            id,
            name,
            status,
            selected: installed && view.chosen_backend() == Some(kind),
            installed,
        };
        rows.push(backend_row(theme, item, install, cx, move |app, cx| {
            app.update_brain_settings(move |s| s.backend = Some(kind), cx)
        }));
    }
    let strategy = s.strategy;
    let chosen = view.chosen_backend();
    if chosen.is_some_and(|kind| brain::backend::unavailable_reason(kind).is_some()) {
        return rows;
    }
    let note: gpui::SharedString = match (chosen, strategy) {
        (Some(BackendKind::Codex), strategy) => ui::i18n::format(
            "settings.brain_strategy_note_codex",
            &[(
                "effort",
                brain::backend::effort_for(BackendKind::Codex, strategy, true)
                    .unwrap_or_default()
                    .to_string(),
            )],
        )
        .into(),
        (_, Strategy::Speed) => t("settings.brain_strategy_note_speed").into(),
        (_, Strategy::Auto) => t("settings.brain_strategy_note_auto").into(),
        (_, Strategy::Quality) => t("settings.brain_strategy_note_quality").into(),
    };
    // OpenCode 没有可随策略调的参数，只按选的模型跑。
    let show_strategy = chosen != Some(BackendKind::OpenCode);
    rows.extend(show_strategy.then(|| {
        row(
            theme,
            t("settings.brain_strategy"),
            Some(note.clone()),
            segmented(
                "settings-brain-strategy",
                vec![
                    seg(
                        "settings-brain-strategy-speed",
                        t("settings.brain_strategy_speed"),
                        strategy == Strategy::Speed,
                        |s| s.strategy = Strategy::Speed,
                    ),
                    seg(
                        "settings-brain-strategy-auto",
                        t("settings.brain_strategy_auto"),
                        strategy == Strategy::Auto,
                        |s| s.strategy = Strategy::Auto,
                    ),
                    seg(
                        "settings-brain-strategy-quality",
                        t("settings.brain_strategy_quality"),
                        strategy == Strategy::Quality,
                        |s| s.strategy = Strategy::Quality,
                    ),
                ],
                cx,
            ),
        )
        .into_any_element()
    }));
    match chosen {
        Some(kind @ (BackendKind::Codex | BackendKind::OpenCode)) => {
            rows.push(super::backend_models::model_row(
                theme, view, kind, true, cx,
            ));
            rows.push(super::backend_models::model_row(
                theme, view, kind, false, cx,
            ));
        }
        _ => {
            rows.push(model_row(theme, true, s.main_model.clone(), true, cx));
            rows.push(model_row(theme, true, s.approval_model.clone(), false, cx));
        }
    }
    rows
}

/// Claude Code 的模型：固定的三个别名加“跟随策略”。
fn model_row(
    theme: Theme,
    claude: bool,
    current: Option<String>,
    main: bool,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    let (title, hint) = if main {
        (
            "settings.brain_main_model",
            "settings.brain_main_model_hint",
        )
    } else {
        (
            "settings.brain_approval_model",
            "settings.brain_approval_model_hint",
        )
    };
    if !claude {
        return row(
            theme,
            t(title),
            Some(t("settings.brain_model_cli_default").into()),
            div(),
        )
        .into_any_element();
    }
    let ids: [&'static str; 4] = if main {
        [
            "settings-brain-main-model-follow",
            "settings-brain-main-model-opus",
            "settings-brain-main-model-sonnet",
            "settings-brain-main-model-haiku",
        ]
    } else {
        [
            "settings-brain-approval-model-follow",
            "settings-brain-approval-model-opus",
            "settings-brain-approval-model-sonnet",
            "settings-brain-approval-model-haiku",
        ]
    };
    let choices: [(Option<&'static str>, &'static str); 4] = [
        (None, t("settings.brain_model_follow_strategy")),
        (Some("opus"), "Opus"),
        (Some("sonnet"), "Sonnet"),
        (Some("haiku"), "Haiku"),
    ];
    let options = ids
        .into_iter()
        .zip(choices)
        .map(|(id, (alias, label))| {
            seg(id, label, current.as_deref() == alias, move |s| {
                let value = alias.map(str::to_string);
                if main {
                    s.main_model = value;
                } else {
                    s.approval_model = value;
                }
            })
        })
        .collect();
    let group = if main {
        "settings-brain-main-model"
    } else {
        "settings-brain-approval-model"
    };
    row(
        theme,
        t(title),
        Some(t(hint).into()),
        segmented(group, options, cx),
    )
    .into_any_element()
}
