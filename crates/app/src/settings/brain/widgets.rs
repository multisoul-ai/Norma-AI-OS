//! 大脑设置页的小部件：表单行、分段按钮、选项卡片、标签。

use crate::app::MultisoulApp;
use crate::settings::typography::{control_label, SettingsText, TextRole};
use gpui::{div, prelude::*, px, rgba, Context, Div, SharedString};
use ui::component::button::Button;
use ui::component::{Disableable, Selectable};
use ui::Theme;

/// 左侧标签、右侧控件的一行，与原型的表单一致。
pub(super) fn form_row(theme: Theme, label: &'static str, control: Div) -> Div {
    div()
        .w_full()
        .flex()
        .flex_row()
        .items_start()
        .gap(px(16.0))
        .child(
            div()
                .w(px(170.0))
                .flex_none()
                .pt(px(6.0))
                .settings_text(TextRole::Helper, theme)
                .child(label),
        )
        .child(div().flex_1().min_w(px(0.0)).child(control))
}

pub(super) fn note(theme: Theme, text: impl Into<SharedString>) -> Div {
    div()
        .pt(px(8.0))
        .pl(px(10.0))
        .max_w(px(560.0))
        .border_l_2()
        .border_color(rgba(theme.node_stroke))
        .settings_text(TextRole::Helper, theme)
        .child(text.into())
}

pub(super) fn wrap_row() -> Div {
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
}

/// 分段按钮里的一个选项。
pub(super) fn segment(
    id: &'static str,
    label: &'static str,
    selected: bool,
    disabled: bool,
    cx: &mut Context<MultisoulApp>,
    on_click: impl Fn(&mut MultisoulApp, &mut Context<MultisoulApp>) + 'static,
) -> Button {
    Button::new(id)
        .child(control_label(label))
        .selected(selected)
        .disabled(disabled)
        .on_click(cx.listener(move |app, _, _, cx| on_click(app, cx)))
}

/// 后端卡片：名称加一行状态，已装才可选。
pub(super) fn backend_card(
    theme: Theme,
    id: &'static str,
    name: &'static str,
    status: String,
    selected: bool,
    installed: bool,
    cx: &mut Context<MultisoulApp>,
    on_click: impl Fn(&mut MultisoulApp, &mut Context<MultisoulApp>) + 'static,
) -> impl IntoElement {
    let border = if selected {
        theme.accent
    } else {
        theme.node_stroke
    };
    div()
        .id(id)
        .w(px(168.0))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .px(px(12.0))
        .py(px(9.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(rgba(border))
        .bg(rgba(if selected {
            theme.panel_high
        } else {
            theme.panel
        }))
        .when(installed, |card| {
            card.cursor_pointer()
                .hover(|style| style.border_color(rgba(theme.accent)))
                .on_click(cx.listener(move |app, _, _, cx| on_click(app, cx)))
        })
        .when(!installed, |card| card.opacity(0.55))
        .child(div().settings_text(TextRole::RowTitle, theme).child(name))
        .child(div().settings_text(TextRole::Helper, theme).child(status))
}

/// 只读的标签，用来列出“始终先问你”的几类操作。
pub(super) fn chip(theme: Theme, label: &'static str) -> Div {
    div()
        .px(px(9.0))
        .py(px(3.0))
        .rounded(px(7.0))
        .bg(rgba(theme.panel))
        .border_1()
        .border_color(rgba(theme.node_stroke))
        .settings_text(TextRole::Badge, theme)
        .child(label)
}
