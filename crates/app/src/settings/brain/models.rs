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

/// 状态卡里显示的主会话模型：只有 Claude Code 能选，其他后端跟随各自 CLI。
pub(super) fn model_name(view: &BrainSettingsView) -> String {
    let kind = view.chosen_backend();
    if kind != Some(BackendKind::ClaudeCode) {
        return t("settings.brain_model_cli").to_string();
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
        let installed = view.installed.contains(&kind);
        let packaged = crate::app::brain_adapter_package(kind).is_some();
        let failed = view
            .install_error
            .as_ref()
            .filter(|(failed, _)| *failed == kind)
            .map(|(_, reason)| reason.clone());
        let status = if installed {
            format!("{} · {program}", t("settings.brain_backend_installed"))
        } else if let Some(reason) = failed {
            ui::i18n::format("settings.brain_install_failed", &[("reason", reason)])
        } else if packaged {
            t("settings.brain_backend_no_adapter").to_string()
        } else {
            t("settings.brain_opencode_missing").to_string()
        };
        let install = (packaged && !installed).then(|| {
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
    let note_key = match strategy {
        Strategy::Speed => "settings.brain_strategy_note_speed",
        Strategy::Auto => "settings.brain_strategy_note_auto",
        Strategy::Quality => "settings.brain_strategy_note_quality",
    };
    rows.push(
        row(
            theme,
            t("settings.brain_strategy"),
            Some(t(note_key).into()),
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
        .into_any_element(),
    );
    let claude = view.chosen_backend() == Some(BackendKind::ClaudeCode);
    rows.push(model_row(theme, claude, s.main_model.clone(), true, cx));
    rows.push(model_row(
        theme,
        claude,
        s.approval_model.clone(),
        false,
        cx,
    ));
    rows
}

/// 模型只对 Claude Code 可选（别名 opus、sonnet、haiku）；其他后端跟随各自 CLI。
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
