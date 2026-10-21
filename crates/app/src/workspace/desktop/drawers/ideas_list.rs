//! 左栏「想法」视图：待办、已指派两组，完成的只计数；指派菜单就地展开在行下方。
//!
//! 设计注记（apple-design / material-menus）：
//! - 行 52px，主动作单一（指派）；hover、按下、键盘焦点、菜单展开四态都有可见反馈，高频操作不加动画。
//! - 菜单就地展开而不是浮层：抽屉很窄，浮层会遮住画布；展开后 ↑/↓ 选择、回车确认、Esc 收回。
//! - 删除可撤销（6 秒），不弹确认框；派发失败时在列表底部给出原因。
//! - 状态同时用文字和颜色表达；拖到卡片的直接操作在后续切片加入。

use super::super::ideas::IdeaTargetOption;
use super::super::ideas_rows::{AssignedRow, IdeaStatus, TodoRow};
use super::super::overview_rows::{overview_feedback, TONE_RUNNING};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, ElementId, KeyDownEvent, Stateful};
use ui::component::button::{Button, ButtonVariants};
use ui::component::StyledExt;

const GROUP_TITLE: u32 = 0xa9c3daff;
const COPY_PRIMARY: u32 = 0xeef6ffff;
const COPY_SECONDARY: u32 = 0xa9c0d5ff;
const GROUP_RULE: u32 = 0xcde8ff14;
const NOTICE: u32 = 0xf6dfaaff;

impl MultisoulApp {
    pub(super) fn render_left_ideas_body(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let focus = self.left_ideas_focus(cx);
        let screen = self.ideas_screen(cx);
        let feedback = overview_feedback(theme);
        let cursor = self.desktop_workspace.ideas_cursor;
        let todo_count = screen.todo.len();
        let menu_open = self.desktop_workspace.ideas_menu.clone();
        let options = if menu_open.is_some() {
            self.idea_target_options(cx)
        } else {
            Vec::new()
        };
        let areas = self.desktop_workspace.expanded_area_id.clone();
        div()
            .id("desktop-ideas")
            .debug_selector(|| "desktop-ideas".to_string())
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .tab_index(0)
            .track_focus(&focus)
            .border_1()
            .border_color(rgba(0x00000000))
            .focus(move |style| style.border_color(rgba(feedback.focus)))
            .on_key_down(cx.listener(|app, event: &KeyDownEvent, window, cx| {
                if app.handle_ideas_key(
                    event.keystroke.key.as_str(),
                    event.keystroke.modifiers.modified(),
                    window,
                    cx,
                ) {
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .px(px(20.0))
                    .pt(px(16.0))
                    .pb(px(8.0))
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .font_semibold()
                            .text_size(px(14.0))
                            .text_color(rgba(COPY_PRIMARY))
                            .child("想法清单"),
                    )
                    .child(
                        div()
                            .id("desktop-ideas-subtitle")
                            .text_size(px(11.0))
                            .text_color(rgba(COPY_SECONDARY))
                            .child(screen.subtitle.clone()),
                    ),
            )
            .child(
                div()
                    .id("desktop-ideas-list")
                    .debug_selector(|| "desktop-ideas-list".to_string())
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .px(px(8.0))
                    .pb(px(12.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .when(
                        screen.todo.is_empty() && screen.assigned.is_empty(),
                        |list| list.child(empty_state(screen.done_count)),
                    )
                    .when(!screen.todo.is_empty(), |list| {
                        list.child(group_heading("待办", Some(todo_count)))
                            .children(screen.todo.iter().enumerate().map(|(index, row)| {
                                let focused = cursor == Some(index);
                                let open = menu_open.as_deref() == Some(row.id.as_str());
                                self.render_todo_row(
                                    row,
                                    focused,
                                    open,
                                    &options,
                                    areas.clone(),
                                    feedback,
                                    cx,
                                )
                            }))
                    })
                    .when(!screen.assigned.is_empty(), |list| {
                        list.child(div().h(px(1.0)).mt(px(4.0)).bg(rgba(GROUP_RULE)))
                            .child(group_heading("已指派", Some(screen.assigned.len())))
                            .children(screen.assigned.iter().enumerate().map(|(index, row)| {
                                let focused = cursor == Some(todo_count + index);
                                self.render_assigned_row(row, focused, feedback, cx)
                            }))
                    })
                    .when(screen.done_count > 0 && !screen.todo.is_empty(), |list| {
                        list.child(
                            div()
                                .px(px(8.0))
                                .pt(px(8.0))
                                .text_size(px(11.0))
                                .text_color(rgba(COPY_SECONDARY))
                                .child(format!("已完成 {} 条", screen.done_count)),
                        )
                    }),
            )
            .when_some(
                self.desktop_workspace.ideas_notice.clone(),
                |body, notice| {
                    body.child(
                        div()
                            .id("desktop-ideas-notice")
                            .mx(px(12.0))
                            .mb(px(8.0))
                            .px(px(10.0))
                            .py(px(8.0))
                            .rounded(px(8.0))
                            .bg(rgba(0xf2c77326))
                            .text_size(px(12.0))
                            .text_color(rgba(NOTICE))
                            .child(notice),
                    )
                },
            )
            .when_some(self.desktop_workspace.ideas_undo.clone(), |body, undo| {
                body.child(
                    div()
                        .id("desktop-ideas-undo")
                        .mx(px(12.0))
                        .mb(px(10.0))
                        .px(px(10.0))
                        .h(px(36.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .rounded(px(8.0))
                        .bg(rgba(0x8fcaff1f))
                        .text_size(px(12.0))
                        .text_color(rgba(COPY_PRIMARY))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .overflow_hidden()
                                .child(format!("已删除「{}」", undo.title)),
                        )
                        .child(
                            Button::new("desktop-ideas-undo-button")
                                .ghost()
                                .child("撤销")
                                .on_click(cx.listener(|app, _, _, cx| {
                                    cx.stop_propagation();
                                    app.undo_delete_idea(cx);
                                })),
                        ),
                )
            })
    }

    fn render_todo_row(
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
        let assign_id = row.id.clone();
        let delete_id = row.id.clone();
        let hover = feedback.hover;
        let pressed = feedback.pressed;
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .id(ElementId::Name(element_id.clone().into()))
                    .debug_selector(move || element_id.clone())
                    .w_full()
                    .min_h(px(52.0))
                    .px(px(8.0))
                    .py(px(6.0))
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
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .child(
                                div()
                                    .font_medium()
                                    .text_size(px(13.0))
                                    .text_color(rgba(COPY_PRIMARY))
                                    .child(row.title.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgba(COPY_SECONDARY))
                                    .child(row.meta.clone()),
                            ),
                    )
                    .child(
                        Button::new(ElementId::Name(format!("idea-assign-{assign_id}").into()))
                            .ghost()
                            .tooltip("指派给智能体 · 回车")
                            .child(if menu_open {
                                "指派 ▴"
                            } else {
                                "指派 ▾"
                            })
                            .on_click(cx.listener(move |app, _, _, cx| {
                                cx.stop_propagation();
                                app.toggle_idea_menu(&assign_id, cx);
                            })),
                    )
                    .child(
                        Button::new(ElementId::Name(format!("idea-delete-{delete_id}").into()))
                            .ghost()
                            .tooltip("删除 · Delete")
                            .child("×")
                            .on_click(cx.listener(move |app, _, _, cx| {
                                cx.stop_propagation();
                                app.delete_idea(&delete_id, cx);
                            })),
                    ),
            )
            .when(menu_open, |column| {
                column.child(self.render_idea_menu(&row.id, options, expanded_area, feedback, cx))
            })
    }

    fn render_assigned_row(
        &self,
        row: &AssignedRow,
        focused: bool,
        feedback: super::super::overview_rows::OverviewFeedback,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let element_id = format!("desktop-idea-assigned-{}", row.id);
        let hover = feedback.hover;
        let pressed = feedback.pressed;
        let locate = row.area_id.clone().zip(row.node_id);
        let status = match &row.status {
            IdeaStatus::Sending => "投递中…".to_owned(),
            IdeaStatus::AssignedTo(name) => format!("已交给 {name}"),
        };
        div()
            .id(ElementId::Name(element_id.clone().into()))
            .debug_selector(move || element_id.clone())
            .w_full()
            .min_h(px(48.0))
            .px(px(8.0))
            .py(px(6.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .rounded(px(9.0))
            .border_1()
            .border_color(rgba(if focused { feedback.focus } else { 0x00000000 }))
            .when(locate.is_some(), |row| {
                row.cursor_pointer()
                    .hover(move |style| style.bg(rgba(hover)))
                    .active(move |style| style.bg(rgba(pressed)))
            })
            .on_click(cx.listener(move |app, _, window, cx| {
                cx.stop_propagation();
                if let Some((area_id, node_id)) = locate.clone() {
                    app.locate_desktop_agent(&area_id, node_id, window, cx);
                }
            }))
            .child(
                div()
                    .w(px(8.0))
                    .h(px(8.0))
                    .flex_none()
                    .rounded_full()
                    .bg(rgba(TONE_RUNNING)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgba(0xc5d9eaff))
                            .child(row.title.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgba(COPY_SECONDARY))
                            .child(status),
                    ),
            )
    }
}

fn group_heading(title: &'static str, count: Option<usize>) -> Div {
    div()
        .h(px(30.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .text_size(px(11.0))
                .font_semibold()
                .text_color(rgba(GROUP_TITLE))
                .child(title),
        )
        .when_some(count, |heading, count| {
            heading.child(
                div()
                    .min_w(px(18.0))
                    .h(px(18.0))
                    .px(px(5.0))
                    .rounded(px(9.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgba(0x8fcaff2e))
                    .text_size(px(11.0))
                    .font_semibold()
                    .text_color(rgba(0xd4eaffff))
                    .child(count.to_string()),
            )
        })
}

fn empty_state(done: usize) -> Stateful<Div> {
    div()
        .id("desktop-ideas-empty")
        .py(px(32.0))
        .px(px(18.0))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(8.0))
        .text_size(px(12.0))
        .text_color(rgba(COPY_SECONDARY))
        .child(
            div()
                .font_semibold()
                .text_size(px(13.0))
                .text_color(rgba(COPY_PRIMARY))
                .child(if done > 0 {
                    "想法都已完成"
                } else {
                    "还没有想法"
                }),
        )
        .child("对 Norma 说“记一下：给设置页加深色主题”，她会帮你记下。")
}
