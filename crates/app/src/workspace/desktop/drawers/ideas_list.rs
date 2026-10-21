//! 左栏「想法」视图：待办、已指派两组，完成的只计数；指派菜单就地展开在行下方。
//!
//! 设计注记（apple-design / material-menus）：
//! - 行 52px，主动作单一（指派）；hover、按下、键盘焦点、菜单展开四态都有可见反馈，高频操作不加动画。
//! - 菜单就地展开而不是浮层：抽屉很窄，浮层会遮住画布；展开后 ↑/↓ 选择、回车确认、Esc 收回。
//! - 删除可撤销（6 秒），不弹确认框；派发失败时在列表底部给出原因。
//! - 状态同时用文字和颜色表达；拖到卡片的直接操作在后续切片加入。

use super::super::overview_layout::OverviewGroup;
use super::super::overview_rows::overview_feedback;
use super::ideas_row::{idea_pill, idea_sub_heading};
use super::overview_list::group_empty;
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, ElementId, KeyDownEvent, Stateful};
use ui::component::button::{Button, ButtonVariants};
use ui::component::input::Input;
use ui::component::Sizable;

const COPY_PRIMARY: u32 = 0xeef6ffff;
const COPY_SECONDARY: u32 = 0xa9c0d5ff;
const GROUP_RULE: u32 = 0xcde8ff14;
const NOTICE: u32 = 0xf6dfaaff;

impl MultisoulApp {
    /// 想法栏的内容区（栏标题与折叠由概览统一绘制）：清单、添加输入、提示与撤销。
    pub(super) fn render_ideas_group_body(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let focus = self.left_ideas_focus(cx);
        let screen = self.ideas_screen(cx);
        let feedback = overview_feedback(theme);
        let cursor = self.desktop_workspace.ideas_cursor;
        let todo_count = screen.todo.len();
        // 条数变多说明有新想法：滚到末尾，让刚记下的那条可见。
        let scroll = self.ideas_scroll.clone();
        if self
            .desktop_workspace
            .ideas_seen_todo
            .is_some_and(|seen| todo_count > seen)
        {
            // 按下标滚到最后一条待办：下标在布局时对照当前内容解析，不依赖上一帧的滚动范围。
            let heading = usize::from(!screen.assigned.is_empty());
            scroll.scroll_to_item(heading + todo_count.saturating_sub(1));
        }
        self.desktop_workspace.ideas_seen_todo = Some(todo_count);
        if self.desktop_workspace.ideas_menu_reveal_frames > 0 {
            if let Some(position) = self
                .desktop_workspace
                .ideas_menu
                .as_deref()
                .and_then(|id| screen.todo.iter().position(|row| row.id == id))
            {
                let heading = usize::from(!screen.assigned.is_empty());
                scroll.scroll_to_item(heading + position);
                self.desktop_workspace.ideas_menu_reveal_frames -= 1;
                cx.notify();
            } else {
                self.desktop_workspace.ideas_menu_reveal_frames = 0;
            }
        }
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
                    .id("desktop-ideas-list")
                    .track_scroll(&scroll)
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
                        |list| list.child(group_empty(OverviewGroup::Ideas)),
                    )
                    .when(!screen.todo.is_empty(), |list| {
                        list.when(!screen.assigned.is_empty(), |list| {
                            list.child(idea_sub_heading(format!("待办 {todo_count}")))
                        })
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
                        let collapsed = self.desktop_workspace.ideas_assigned_collapsed;
                        list.child(div().h(px(1.0)).mt(px(4.0)).bg(rgba(GROUP_RULE)))
                            .child(self.assigned_heading(
                                screen.assigned.len(),
                                collapsed,
                                feedback,
                                cx,
                            ))
                            .when(!collapsed, |list| {
                                list.children(screen.assigned.iter().enumerate().map(
                                    |(index, row)| {
                                        let focused = cursor == Some(todo_count + index);
                                        self.render_assigned_row(row, focused, feedback, cx)
                                    },
                                ))
                            })
                    })
                    .when(!screen.done.is_empty(), |list| {
                        let show = self.desktop_workspace.ideas_show_done;
                        list.child(
                            div()
                                .id("desktop-ideas-done-toggle")
                                .mt(px(8.0))
                                .h(px(32.0))
                                .px(px(8.0))
                                .flex()
                                .items_center()
                                .rounded(px(8.0))
                                .cursor_pointer()
                                .hover(move |style| style.bg(rgba(feedback.hover)))
                                .active(move |style| style.bg(rgba(feedback.pressed)))
                                .text_size(px(11.0))
                                .text_color(rgba(COPY_SECONDARY))
                                .on_click(cx.listener(|app, _, _, cx| {
                                    cx.stop_propagation();
                                    app.desktop_workspace.ideas_show_done =
                                        !app.desktop_workspace.ideas_show_done;
                                    cx.notify();
                                }))
                                .child(format!(
                                    "{} 已完成 {} 条",
                                    if show { "▾" } else { "▸" },
                                    screen.done.len()
                                )),
                        )
                        .when(show, |list| {
                            list.children(screen.done.iter().map(|row| {
                                div()
                                    .id(ElementId::Name(
                                        format!("desktop-idea-done-{}", row.id).into(),
                                    ))
                                    .px(px(8.0))
                                    .py(px(5.0))
                                    .text_size(px(12.0))
                                    .text_color(rgba(COPY_SECONDARY))
                                    .child(format!("✓ {}", row.title))
                            }))
                        })
                    }),
            )
            .when_some(self.desktop_workspace.ideas_input.clone(), |body, input| {
                body.child(
                    div()
                        .id("desktop-ideas-add")
                        .debug_selector(|| "desktop-ideas-add".to_string())
                        .flex_none()
                        .mx(px(8.0))
                        .mt(px(4.0))
                        .mb(px(2.0))
                        .py(px(3.0))
                        .pr(px(3.0))
                        .pl(px(10.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .rounded(px(9.0))
                        .border_1()
                        .border_color(rgba(0x83c4ff40))
                        .bg(rgba(0x0b1b2d99))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .h(px(28.0))
                                .flex()
                                .items_center()
                                .text_size(px(12.0))
                                .text_color(rgba(COPY_PRIMARY))
                                .child(Input::new(&input).small().appearance(false)),
                        )
                        .child(idea_pill("desktop-ideas-add-button", "添加").on_click(
                            cx.listener(|app, _, window, cx| {
                                cx.stop_propagation();
                                app.add_idea_from_input(window, cx);
                            }),
                        )),
                )
            })
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
}
