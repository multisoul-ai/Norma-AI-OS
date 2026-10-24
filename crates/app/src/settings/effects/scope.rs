//! 范围：工作空间 / 项目，与壁纸页的 `scope_controls` 同构。

use super::page::EffectsPageData;
use crate::app::MultisoulApp;
use crate::settings::typography::{control_label, SettingsText, TextRole};
use gpui::{div, prelude::*, px, Context, Div};
use ui::component::{
    button::{Button, ButtonGroup},
    Disableable, Selectable,
};
use ui::Theme;

pub(super) fn scope_section(
    theme: Theme,
    data: &EffectsPageData,
    compact: bool,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let row = div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .when(compact, |row| row.flex_col().items_start());
    if data.workspace_name.is_none() {
        return row.child(
            div()
                .settings_text(TextRole::Helper, theme)
                .child(ui::i18n::text("effects.scope_hint")),
        );
    }
    let project = data.state.edit_project && data.can_edit_project;
    row.child(
        div()
            .min_w(px(0.0))
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .settings_text(TextRole::Eyebrow, theme)
                    .child(ui::i18n::text("settings.personalize")),
            )
            .child(
                div()
                    .settings_font(TextRole::Headline)
                    .truncate()
                    .child(data.scope_label()),
            ),
    )
    .child(
        ButtonGroup::new("effects-scope").outline().children([
            Button::new("effects-scope-workspace")
                .w(px(122.0))
                .rounded(px(18.0))
                .child(control_label(if project {
                    ui::i18n::text("common.workspace")
                } else {
                    ui::i18n::text("settings.workspace_selected")
                }))
                .selected(!project)
                .on_click(cx.listener(|app, _, _, cx| app.set_effects_scope(false, cx))),
            Button::new("effects-scope-project")
                .w(px(108.0))
                .rounded(px(18.0))
                .child(control_label(if project {
                    ui::i18n::text("settings.project_selected")
                } else {
                    ui::i18n::text("settings.project")
                }))
                .selected(project)
                .disabled(!data.can_edit_project)
                .tooltip(ui::i18n::text("settings.project_wallpaper_scope"))
                .on_click(cx.listener(|app, _, _, cx| app.set_effects_scope(true, cx))),
        ]),
    )
}
