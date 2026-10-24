//! Codex、OpenCode 的模型选择：模型列表从后端读出，用下拉菜单选择，按后端分别保存。

use super::t;
use super::widgets::row;
use crate::app::{BrainSettings, BrainSettingsView, ModelList, MultisoulApp};
use crate::settings::typography::{control_label, SettingsText, TextRole};
use brain::{BackendKind, ModelChoice};
use gpui::{div, prelude::*, px, AnyElement, Context, SharedString};
use ui::component::button::Button;
use ui::component::menu::{DropdownMenu, PopupMenuItem};
use ui::Theme;

/// 主会话或审批会话一行：标题、说明，右边是模型下拉菜单。
pub(super) fn model_row(
    theme: Theme,
    view: &BrainSettingsView,
    kind: BackendKind,
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
    let pair = view.settings.models_for(kind);
    let current = if main { pair.main } else { pair.approval };
    let control = match view.model_lists.get(&kind) {
        Some(ModelList::Ready(models)) if !models.is_empty() => {
            dropdown(kind, main, current, models.clone(), cx).into_any_element()
        }
        Some(ModelList::Failed(reason)) => helper(
            theme,
            ui::i18n::format(
                "settings.brain_models_failed",
                &[("reason", reason.clone())],
            ),
        ),
        Some(ModelList::Ready(_)) => helper(theme, t("settings.brain_model_cli_default")),
        _ => helper(theme, t("settings.brain_models_loading")),
    };
    row(theme, t(title), Some(t(hint).into()), control).into_any_element()
}

fn helper(theme: Theme, text: impl Into<SharedString>) -> AnyElement {
    div()
        .max_w(px(260.0))
        .settings_text(TextRole::Helper, theme)
        .child(text.into())
        .into_any_element()
}

/// 列表里模型的显示名；没找到时直接显示 id。
pub(super) fn display_name(models: Option<&ModelList>, id: &str) -> String {
    match models {
        Some(ModelList::Ready(list)) => list
            .iter()
            .find(|model| model.id == id)
            .map_or_else(|| id.to_string(), |model| model.name.clone()),
        _ => id.to_string(),
    }
}

fn set_model(settings: &mut BrainSettings, kind: BackendKind, main: bool, value: Option<String>) {
    let pair = settings.backend_models.entry(kind).or_default();
    if main {
        pair.main = value;
    } else {
        pair.approval = value;
    }
}

/// 下拉菜单：第一项“CLI 默认”，其余是后端报出的模型；当前值打勾。
fn dropdown(
    kind: BackendKind,
    main: bool,
    current: Option<String>,
    models: Vec<ModelChoice>,
    cx: &mut Context<MultisoulApp>,
) -> impl IntoElement {
    let default_label = t("settings.brain_model_cli_default_short");
    let label = current.as_ref().map_or_else(
        || default_label.to_string(),
        |id| {
            models
                .iter()
                .find(|model| &model.id == id)
                .map_or_else(|| id.clone(), |model| model.name.clone())
        },
    );
    let id = if main {
        "settings-brain-main-model-menu"
    } else {
        "settings-brain-approval-model-menu"
    };
    let app = cx.weak_entity();
    Button::new(id)
        .outline()
        .dropdown_caret(true)
        .min_w(px(200.0))
        .child(control_label(label))
        .dropdown_menu(move |menu, _window, _cx| {
            let pick = |value: Option<String>| {
                let app = app.clone();
                move |_: &gpui::ClickEvent, _: &mut gpui::Window, cx: &mut gpui::App| {
                    let value = value.clone();
                    let _ = app.update(cx, |app, cx| {
                        app.update_brain_settings(|s| set_model(s, kind, main, value), cx)
                    });
                }
            };
            let mut menu = menu.max_h(px(360.0)).scrollable(true).item(
                PopupMenuItem::new(default_label)
                    .checked(current.is_none())
                    .on_click(pick(None)),
            );
            for model in &models {
                menu = menu.item(
                    PopupMenuItem::new(model.name.clone())
                        .checked(current.as_deref() == Some(model.id.as_str()))
                        .on_click(pick(Some(model.id.clone()))),
                );
            }
            menu
        })
}
