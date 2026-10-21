//! 想法的指派菜单：悬浮在「指派」按钮下方（对照原型 `.idea-menu`），↑/↓ 选择、回车确认、Esc 收回。

use super::super::ideas::{IdeaTargetOption, IdeaUiTarget};
use super::super::overview_rows::OverviewFeedback;
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, ElementId, Stateful, Window};

const GROUP_TITLE: u32 = 0xa9c3daff;
const GROUP_RULE: u32 = 0xcde8ff14;
const MENU_FILL: u32 = 0x0d1d2ff2;
const MENU_BORDER: u32 = 0x83c4ff66;

impl MultisoulApp {
    pub(super) fn render_idea_menu(
        &self,
        idea_id: &str,
        options: &[IdeaTargetOption],
        expanded_area: Option<String>,
        feedback: OverviewFeedback,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let cursor = self.desktop_workspace.ideas_menu_cursor;
        let new_area =
            expanded_area.or_else(|| options.first().map(|option| option.area_id.clone()));
        let total = options.len();
        div()
            .id("desktop-idea-menu")
            .debug_selector(|| "desktop-idea-menu".to_string())
            .w(px(236.0))
            .p(px(6.0))
            .flex()
            .flex_col()
            .rounded(px(12.0))
            .shadow(vec![gpui::BoxShadow {
                color: rgba(0x000000bb).into(),
                offset: gpui::point(px(0.0), px(18.0)),
                blur_radius: px(44.0),
                spread_radius: px(0.0),
            }])
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .border_1()
            .border_color(rgba(MENU_BORDER))
            .bg(rgba(MENU_FILL))
            .child(
                div()
                    .px(px(10.0))
                    .pt(px(4.0))
                    .pb(px(6.0))
                    .text_size(px(11.0))
                    .text_color(rgba(GROUP_TITLE))
                    .child("交给谁来做"),
            )
            .children(options.iter().enumerate().map(|(index, option)| {
                let target = IdeaUiTarget::Existing {
                    area_id: option.area_id.clone(),
                    node_id: option.node_id,
                };
                let id = idea_id.to_owned();
                menu_item(
                    MenuItem {
                        id: format!("desktop-idea-target-{index}"),
                        label: option.name.clone(),
                        detail: Some(option.status.to_owned()),
                        accent: false,
                        dot: if option.running {
                            Dot::Running
                        } else {
                            Dot::Idle
                        },
                        selected: cursor == index,
                    },
                    feedback,
                    cx.listener(move |app, event: &gpui::ClickEvent, window, cx| {
                        cx.stop_propagation();
                        let from = Some(click_point(event));
                        app.dispatch_idea(id.clone(), target.clone(), from, window, cx);
                    }),
                )
            }))
            .child(
                div()
                    .h(px(1.0))
                    .mx(px(4.0))
                    .my(px(4.0))
                    .bg(rgba(GROUP_RULE)),
            )
            .child({
                let id = idea_id.to_owned();
                menu_item(
                    MenuItem {
                        id: "desktop-idea-target-new".to_owned(),
                        label: "新建智能体来完成".to_owned(),
                        detail: None,
                        accent: true,
                        dot: Dot::NewAgent,
                        selected: cursor == total,
                    },
                    feedback,
                    cx.listener(move |app, event: &gpui::ClickEvent, window, cx| {
                        cx.stop_propagation();
                        if let Some(area_id) = new_area.clone() {
                            let from = Some(click_point(event));
                            let target = IdeaUiTarget::New { area_id };
                            app.dispatch_idea(id.clone(), target, from, window, cx);
                        }
                    }),
                )
            })
    }
}

/// 菜单点击的窗口坐标，流星从这里起飞。
fn click_point(event: &gpui::ClickEvent) -> (f32, f32) {
    let at = event.position();
    (f32::from(at.x), f32::from(at.y))
}

#[derive(Clone, Copy)]
enum Dot {
    Running,
    Idle,
    NewAgent,
}

struct MenuItem {
    id: String,
    dot: Dot,
    label: String,
    detail: Option<String>,
    accent: bool,
    selected: bool,
}

fn menu_item(
    item: MenuItem,
    feedback: super::super::overview_rows::OverviewFeedback,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> Stateful<Div> {
    let MenuItem {
        id,
        dot,
        label,
        detail,
        accent,
        selected,
    } = item;
    let (hover, pressed, focus) = (feedback.hover, feedback.pressed, feedback.focus);
    div()
        .id(ElementId::Name(id.clone().into()))
        .debug_selector(move || id.clone())
        .w_full()
        .h(px(36.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(if selected { focus } else { 0x00000000 }))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(hover)))
        .active(move |style| style.bg(rgba(pressed)))
        .on_click(on_click)
        .child(dot_view(dot))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .text_size(px(13.0))
                .text_color(rgba(if accent { 0xbfe6ffff } else { 0xe8f3fdff }))
                .child(label),
        )
        .when_some(detail, |item, detail| {
            item.child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgba(0x9fb8cfff))
                    .child(detail),
            )
        })
}

/// 行首圆点：运行中青色、空闲灰蓝，「新建」是空心圆环。
fn dot_view(dot: Dot) -> Div {
    let cell = div()
        .w(px(14.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center();
    match dot {
        Dot::Running => cell.child(
            div()
                .w(px(8.0))
                .h(px(8.0))
                .rounded_full()
                .bg(rgba(0x36dce8ff)),
        ),
        Dot::Idle => cell.child(
            div()
                .w(px(8.0))
                .h(px(8.0))
                .rounded_full()
                .bg(rgba(0x7b9dbdff)),
        ),
        Dot::NewAgent => cell.child(
            div()
                .w(px(8.0))
                .h(px(8.0))
                .rounded_full()
                .border_2()
                .border_color(rgba(0x9fd2ffff)),
        ),
    }
}
