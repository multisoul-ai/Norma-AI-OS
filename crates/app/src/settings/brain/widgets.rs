//! 幕僚设置页的小部件，按 Material 3 的列表排布：分组标题 + 一张卡片，卡片里一行一个设置。

use crate::app::MultisoulApp;
use crate::settings::typography::{control_label, SettingsText, TextRole};
use gpui::{div, prelude::*, px, rgba, AnyElement, Context, Div, SharedString, Window};
use ui::component::button::{Button, ButtonGroup, ButtonVariants};
use ui::component::switch::Switch;
use ui::component::{Disableable, Selectable, Sizable as _, Size};
use ui::icons::with_alpha;
use ui::Theme;

/// 卡片里一行的最小高度：标题加一行说明。
const ROW_HEIGHT: f32 = 64.0;

/// 分组：上方一行强调色的小标题，下面一张卡片，行与行之间有分隔线。
pub(super) fn section(theme: Theme, title: &'static str, rows: Vec<AnyElement>) -> Div {
    let mut card = div()
        .w_full()
        .flex()
        .flex_col()
        .rounded(px(16.0))
        .overflow_hidden()
        // 浅色主题里卡片与页面底色接近，靠描边分出层次。
        .border_1()
        .border_color(rgba(with_alpha(theme.text_primary, 0x1f)))
        .bg(rgba(theme.panel_high));
    for (index, row) in rows.into_iter().enumerate() {
        card = card.child(
            div()
                .when(index > 0, |cell| {
                    cell.border_t_1()
                        .border_color(rgba(with_alpha(theme.text_primary, 0x1a)))
                })
                .child(row),
        );
    }
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .pl(px(4.0))
                .settings_text(TextRole::Eyebrow, theme)
                .text_color(rgba(theme.accent))
                .child(title),
        )
        .child(card)
}

/// 一行设置：左边标题与一行说明，右边控件。
pub(super) fn row(
    theme: Theme,
    title: impl Into<SharedString>,
    hint: Option<SharedString>,
    control: impl IntoElement,
) -> Div {
    div()
        .w_full()
        .min_h(px(ROW_HEIGHT))
        .px(px(20.0))
        .py(px(12.0))
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(16.0))
        .child(
            div()
                .flex_1()
                .min_w(px(220.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .settings_text(TextRole::RowTitle, theme)
                        .child(title.into()),
                )
                .when_some(hint, |label, hint| {
                    label.child(div().settings_text(TextRole::Helper, theme).child(hint))
                }),
        )
        .child(div().flex_none().child(control))
}

/// 卡片底部的说明条。
pub(super) fn note(theme: Theme, text: impl Into<SharedString>) -> Div {
    div()
        .w_full()
        .px(px(20.0))
        .py(px(12.0))
        .bg(rgba(theme.panel))
        .settings_text(TextRole::Helper, theme)
        .child(text.into())
}

/// 开关：点击后把新值交给 `apply`。
pub(super) fn switch(
    id: &'static str,
    checked: bool,
    cx: &mut Context<MultisoulApp>,
    apply: impl Fn(&mut MultisoulApp, bool, &mut Context<MultisoulApp>) + 'static,
) -> Switch {
    let app = cx.weak_entity();
    Switch::new(id)
        .checked(checked)
        .with_size(Size::Medium)
        .on_click(move |value, _window: &mut Window, cx| {
            let value = *value;
            let _ = app.update(cx, |app, cx| apply(app, value, cx));
        })
}

/// 分段按钮的一个选项：（元素 id，文字，是否选中，点击后做什么）。
pub(super) type Segment = (
    &'static str,
    &'static str,
    bool,
    Box<dyn Fn(&mut MultisoulApp, &mut Context<MultisoulApp>)>,
);

/// Material 的分段按钮：一组相连的选项，选中的一段着色。
pub(super) fn segmented(
    id: &'static str,
    options: Vec<Segment>,
    cx: &mut Context<MultisoulApp>,
) -> ButtonGroup {
    let buttons = options.into_iter().map(|(id, label, selected, on_click)| {
        Button::new(id)
            .child(control_label(label))
            .selected(selected)
            .on_click(cx.listener(move |app, _, _, cx| on_click(app, cx)))
    });
    ButtonGroup::new(id).outline().children(buttons)
}

/// 后端列表里的一项。
pub(super) struct BackendItem {
    pub id: &'static str,
    pub name: &'static str,
    pub status: String,
    pub selected: bool,
    pub installed: bool,
}

/// 后端的一行：左边单选圈，已装才可选；未装的可带一个安装按钮（只有 Claude Code 能一键装）。
pub(super) fn backend_row(
    theme: Theme,
    item: BackendItem,
    trailing: Option<Button>,
    cx: &mut Context<MultisoulApp>,
    on_click: impl Fn(&mut MultisoulApp, &mut Context<MultisoulApp>) + 'static,
) -> AnyElement {
    let hover = with_alpha(theme.text_primary, 0x10);
    let pressed = with_alpha(theme.text_primary, 0x1c);
    let BackendItem {
        id,
        name,
        status,
        selected,
        installed,
    } = item;
    let ring = if selected {
        theme.accent
    } else {
        theme.text_secondary
    };
    div()
        .id(id)
        .w_full()
        .min_h(px(ROW_HEIGHT))
        .px(px(20.0))
        .py(px(12.0))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(16.0))
        .when(installed, |row| {
            row.cursor_pointer()
                .hover(move |style| style.bg(rgba(hover)))
                .active(move |style| style.bg(rgba(pressed)))
                .on_click(cx.listener(move |app, _, _, cx| on_click(app, cx)))
        })
        .when(!installed, |row| row.opacity(0.6))
        .child(
            div()
                .size(px(20.0))
                .flex_none()
                .rounded_full()
                .border_2()
                .border_color(rgba(ring))
                .flex()
                .items_center()
                .justify_center()
                .when(selected, |dot| {
                    dot.child(div().size(px(10.0)).rounded_full().bg(rgba(theme.accent)))
                }),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(div().settings_text(TextRole::RowTitle, theme).child(name))
                .child(div().settings_text(TextRole::Helper, theme).child(status)),
        )
        .children(trailing)
        .into_any_element()
}

/// 安装适配器的文字按钮。
pub(super) fn text_button(
    id: &'static str,
    label: &'static str,
    disabled: bool,
    cx: &mut Context<MultisoulApp>,
    on_click: impl Fn(&mut MultisoulApp, &mut Context<MultisoulApp>) + 'static,
) -> Button {
    Button::new(id)
        .child(control_label(label))
        .ghost()
        .disabled(disabled)
        .on_click(cx.listener(move |app, _, _, cx| on_click(app, cx)))
}

/// 只读的标签，列出幕僚判断审批时会特别留意的几类操作。
pub(super) fn chip(theme: Theme, label: &'static str) -> Div {
    div()
        .h(px(32.0))
        .px(px(14.0))
        .flex()
        .items_center()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(theme.node_stroke))
        .settings_text(TextRole::Label, theme)
        .child(label)
}
