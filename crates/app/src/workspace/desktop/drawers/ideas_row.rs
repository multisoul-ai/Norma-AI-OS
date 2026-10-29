//! 想法清单的待办行与小部件，样式对照原型 `.idea-row` / `.idea-btn` / `.idea-sub`。
//!
//! - 行 52px：左侧 18px 抓手（六点），中间标题（13px/560，最多两行）加元信息（12px），
//!   右侧「指派」胶囊与 28px 的删除图标；两个操作平时隐藏，悬停、键盘焦点或菜单展开时才显示。
//! - 指派胶囊是整行唯一的主动作：淡蓝底加 1px 内描边，悬停与按下各有更深一档。

use super::super::ideas::IdeaTargetOption;
use super::super::ideas_rows::TodoRow;
use crate::app::MultisoulApp;
use gpui::{
    div, img, prelude::*, px, rgba, svg, Context, Div, ElementId, MouseButton, MouseDownEvent,
    ObjectFit, Stateful,
};
use ui::component::tooltip::Tooltip;

const COPY_PRIMARY: u32 = 0xeef6ffff;
const COPY_SECONDARY: u32 = 0xa9c0d5ff;
const GRIP: u32 = 0x6f8ba3ff;
const GRIP_ACTIVE: u32 = 0xbcd8f0ff;

/// 淡蓝底的主动作胶囊（指派）。
pub(super) fn idea_pill(id: impl Into<ElementId>, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(28.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(4.0))
        .rounded(px(7.0))
        .bg(rgba(0x58adff26))
        .border_1()
        .border_color(rgba(0x83c4ff55))
        .text_size(px(11.0))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(rgba(0xdff0ffff))
        .cursor_pointer()
        .hover(|style| style.bg(rgba(0x58adff40)))
        .active(|style| style.bg(rgba(0x58adff59)))
        .child(label)
}

/// 输入框右下角的圆形发送按钮：有内容时填充强调色并可点，没有内容时只剩淡色箭头且不响应。
pub(super) fn idea_send_button(enabled: bool) -> Stateful<Div> {
    let icon = if enabled { 0x06182aff } else { 0x8ea8bf99 };
    div()
        .id("desktop-ideas-add-button")
        .w(px(32.0))
        .h(px(32.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .when(enabled, |button| {
            button
                .bg(rgba(0x7cc0ffff))
                .cursor_pointer()
                .hover(|style| style.bg(rgba(0x9dd0ffff)))
                .active(|style| style.bg(rgba(0xb8deffff)))
        })
        .when(!enabled, |button| {
            button.border_1().border_color(rgba(0x8fcaff26))
        })
        .child(
            svg()
                .path("icons/arrow-up.svg")
                .w(px(16.0))
                .h(px(16.0))
                .flex_none()
                .text_color(rgba(icon)),
        )
}

/// 28px 的图标按钮（删除）。
fn idea_icon_button(id: impl Into<ElementId>, icon: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .w(px(28.0))
        .h(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(7.0))
        .text_color(rgba(0xbcd8f0ff))
        .cursor_pointer()
        .hover(|style| style.bg(rgba(0x8fcaff2e)).text_color(rgba(0xffffffff)))
        .active(|style| style.bg(rgba(0x8fcaff48)))
        .child(svg().path(icon).w(px(13.0)).h(px(13.0)).flex_none())
}

/// 抓手：与原型一致的 grip 图标（14px），悬停行时提亮。
fn grip(active: bool) -> Div {
    div()
        .w(px(18.0))
        .h(px(28.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgba(if active { GRIP_ACTIVE } else { GRIP }))
        .child(
            svg()
                .path("icons/grip.svg")
                .w(px(14.0))
                .h(px(14.0))
                .flex_none(),
        )
}

/// 链接与图片的小标记：点一下用系统默认方式打开；按下时不触发行的拖动与选中。
fn attachment_chip(id: String) -> Stateful<Div> {
    div()
        .id(ElementId::Name(id.into()))
        .flex_none()
        .flex()
        .items_center()
        .rounded(px(5.0))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

fn link_label(link: &str) -> String {
    let rest = link.split_once("://").map_or(link, |(_, rest)| rest);
    let host = rest.split('/').next().unwrap_or(rest);
    let more = rest.len() > host.len();
    let cut: String = host.chars().take(24).collect();
    if more || cut.len() < host.len() {
        format!("🔗 {cut}…")
    } else {
        format!("🔗 {cut}")
    }
}

impl MultisoulApp {
    /// 待办行里的链接（最多 2 个）与图片缩略图；都没有时不占位。
    fn render_idea_attachments(&self, row: &TodoRow, cx: &mut Context<Self>) -> Option<Div> {
        if row.links.is_empty() && row.image_names.is_empty() {
            return None;
        }
        let store = storage::IdeasStore::new(self.database_paths.app_db_path().to_owned());
        let links = row.links.iter().take(2).map(|link| {
            let target = link.clone();
            attachment_chip(format!("idea-link-{}-{link}", row.id))
                .h(px(20.0))
                .px(px(6.0))
                .bg(rgba(0x58adff1f))
                .text_size(px(11.0))
                .text_color(rgba(0xbfe0ffff))
                .hover(|style| style.bg(rgba(0x58adff3d)))
                .active(|style| style.bg(rgba(0x58adff59)))
                .tooltip({
                    let link = link.clone();
                    move |window, cx| Tooltip::new(link.clone()).build(window, cx)
                })
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.stop_propagation();
                    cx.open_url(&target);
                }))
                .child(link_label(link))
        });
        let images = row.image_names.iter().filter_map(|name| {
            let path = store.image_path(name).ok()?;
            let target = format!("file://{}", path.display());
            Some(
                attachment_chip(format!("idea-image-{}-{name}", row.id))
                    .w(px(24.0))
                    .h(px(24.0))
                    .overflow_hidden()
                    .border_1()
                    .border_color(rgba(0x83c4ff40))
                    .hover(|style| style.border_color(rgba(0x83c4ffa0)))
                    .tooltip(|window, cx| {
                        Tooltip::new(ui::i18n::text("drawers.open_image")).build(window, cx)
                    })
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.stop_propagation();
                        cx.open_url(&target);
                    }))
                    .child(
                        img(path)
                            .w(px(24.0))
                            .h(px(24.0))
                            .object_fit(ObjectFit::Cover),
                    ),
            )
        });
        Some(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(4.0))
                .children(links)
                .children(images),
        )
    }

    pub(super) fn render_todo_row(
        &self,
        row: &TodoRow,
        focused: bool,
        menu_open: bool,
        options: &[IdeaTargetOption],
        expanded_area: Option<String>,
        feedback: super::super::overview_rows::OverviewFeedback,
        cx: &mut Context<Self>,
    ) -> Div {
        let id = row.id.clone();
        let element_id = format!("desktop-idea-row-{}", row.id);
        let group = format!("idea-group-{}", row.id);
        let assign_id = row.id.clone();
        let delete_id = row.id.clone();
        let drag_id = row.id.clone();
        let hover = feedback.hover;
        let pressed = feedback.pressed;
        let show_actions = focused || menu_open;
        let popover = menu_open.then(|| {
            // 菜单悬浮在「指派」按钮下方、右缘对齐；延迟绘制，不被栏的滚动区域裁剪。
            div()
                .absolute()
                .right(px(0.0))
                .top(px(34.0))
                .w(px(0.0))
                .h(px(0.0))
                .child(gpui::deferred(
                    gpui::anchored()
                        .anchor(gpui::Corner::TopRight)
                        .snap_to_window_with_margin(px(8.0))
                        .child(self.render_idea_menu(
                            &row.id,
                            options,
                            expanded_area,
                            feedback,
                            cx,
                        )),
                ))
        });
        let actions = div()
            .relative()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(2.0))
            .when(!show_actions, |actions| actions.invisible())
            .group_hover(group.clone(), |style| style.visible())
            .child(
                idea_pill(
                    ElementId::Name(format!("idea-assign-{assign_id}").into()),
                    if menu_open {
                        ui::i18n::text("drawers.assign_up")
                    } else {
                        ui::i18n::text("drawers.assign_down")
                    },
                )
                .tooltip(|window, cx| {
                    Tooltip::new(ui::i18n::text("drawers.assign_tooltip")).build(window, cx)
                })
                .on_click(cx.listener(move |app, _, window, cx| {
                    cx.stop_propagation();
                    // 鼠标点开菜单后，↑/↓/回车/Esc 也要能用：焦点交给想法栏。
                    app.left_ideas_focus(cx).focus(window);
                    app.toggle_idea_menu(&assign_id, cx);
                })),
            )
            .child(
                idea_icon_button(
                    ElementId::Name(format!("idea-delete-{delete_id}").into()),
                    "icons/close.svg",
                )
                .tooltip(|window, cx| {
                    Tooltip::new(ui::i18n::text("drawers.delete_tooltip")).build(window, cx)
                })
                .on_click(cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    app.delete_idea(&delete_id, cx);
                })),
            );
        div().flex().flex_none().flex_col().child(
            div()
                .id(ElementId::Name(element_id.clone().into()))
                .debug_selector(move || element_id.clone())
                .group(group)
                .w_full()
                .min_h(px(46.0))
                .py(px(6.0))
                .pl(px(4.0))
                .pr(px(6.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .rounded(px(9.0))
                .border_1()
                .border_color(rgba(if focused { feedback.focus } else { 0x00000000 }))
                .bg(rgba(if menu_open { hover } else { feedback.rest }))
                .cursor_pointer()
                .hover(move |style| style.bg(rgba(hover)))
                .active(move |style| style.bg(rgba(pressed)))
                .on_click(cx.listener(move |app, _, window, cx| {
                    cx.stop_propagation();
                    app.select_idea_row(&id, window, cx);
                }))
                .child(
                    div()
                        .id(ElementId::Name(format!("idea-grab-{drag_id}").into()))
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .cursor_grab()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |app, event: &MouseDownEvent, window, cx| {
                                app.begin_idea_drag(&drag_id, event, window, cx);
                            }),
                        )
                        .child(grip(menu_open || focused))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .flex()
                                .flex_col()
                                .gap(px(3.0))
                                .child(
                                    div()
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_size(px(12.0))
                                        .line_height(px(16.0))
                                        .max_h(px(32.0))
                                        .overflow_hidden()
                                        .text_color(rgba(COPY_PRIMARY))
                                        .child(row.title.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(rgba(COPY_SECONDARY))
                                        .child(row.meta.clone()),
                                )
                                .children(self.render_idea_attachments(row, cx)),
                        ),
                )
                .child(actions.children(popover)),
        )
    }
}
