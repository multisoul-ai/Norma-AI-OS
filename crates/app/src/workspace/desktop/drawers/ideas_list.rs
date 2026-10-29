//! 左栏「想法」视图：待办、已指派两组，完成的只计数；指派菜单就地展开在行下方。
//!
//! 设计注记（apple-design / material-menus）：
//! - 行 52px，主动作单一（指派）；hover、按下、键盘焦点、菜单展开四态都有可见反馈，高频操作不加动画。
//! - 菜单就地展开而不是浮层：抽屉很窄，浮层会遮住画布；展开后 ↑/↓ 选择、回车确认、Esc 收回。
//! - 删除可撤销（6 秒），不弹确认框；派发失败时在列表底部给出原因。
//! - 状态同时用文字和颜色表达；拖到卡片的直接操作在后续切片加入。

use super::super::ideas_rows::IdeasTab;
use super::super::overview_rows::overview_feedback;
use super::ideas_row::idea_send_button;
use super::ideas_tabs::{clear_row, tab_empty};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, ElementId, KeyDownEvent, Stateful};
use ui::component::button::{Button, ButtonVariants};
use ui::component::input::{Input, Paste};
use ui::component::Sizable;

impl MultisoulApp {
    /// 想法栏的内容区（栏标题与折叠由概览统一绘制）：清单、添加输入、提示与撤销。
    pub(super) fn render_ideas_group_body(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let focus = self.left_ideas_focus(cx);
        let screen = self.ideas_screen(cx);
        let feedback = overview_feedback(theme);
        let cursor = self.desktop_workspace.ideas_cursor;
        let todo_count = screen.todo.len();
        let tab = self.desktop_workspace.ideas_tab;
        // 条数变多说明有新想法：滚到末尾，让刚记下的那条可见。
        let scroll = self.ideas_scroll.clone();
        if tab == IdeasTab::Todo
            && self
                .desktop_workspace
                .ideas_seen_todo
                .is_some_and(|seen| todo_count > seen)
        {
            self.desktop_workspace.ideas_aux.stick_bottom_frames = 3;
        }
        if tab == IdeasTab::Todo && self.desktop_workspace.ideas_aux.stick_bottom_frames > 0 {
            // 待办在列表末尾追加：连续几帧滚到底部，新记下的那条随即可见。
            self.desktop_workspace.ideas_aux.stick_bottom_frames -= 1;
            scroll.scroll_to_bottom();
            cx.notify();
        }
        self.desktop_workspace.ideas_seen_todo = Some(todo_count);
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
                let modifiers = &event.keystroke.modifiers;
                let alt_only =
                    modifiers.alt && !modifiers.control && !modifiers.platform && !modifiers.shift;
                if app.handle_ideas_key(
                    event.keystroke.key.as_str(),
                    modifiers.modified(),
                    alt_only,
                    window,
                    cx,
                ) {
                    cx.stop_propagation();
                }
            }))
            .child(self.render_ideas_tabs(
                [screen.todo.len(), screen.assigned.len(), screen.done.len()],
                feedback,
                cx,
            ))
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
                    .when(tab == IdeasTab::Todo, |list| {
                        list.when(screen.todo.is_empty(), |list| {
                            list.child(tab_empty(
                                theme,
                                ui::i18n::text("drawers.no_todo"),
                                ui::i18n::text("drawers.add_idea_hint"),
                            ))
                        })
                        .when(!screen.todo.is_empty(), |list| {
                            list.child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .on_children_prepainted({
                                        let ids: Vec<String> =
                                            screen.todo.iter().map(|row| row.id.clone()).collect();
                                        let cell =
                                            self.desktop_workspace.ideas_aux.row_bounds.clone();
                                        move |bounds, _window, _cx| {
                                            if let Ok(mut rows) = cell.lock() {
                                                *rows = ids.iter().cloned().zip(bounds).collect();
                                            }
                                        }
                                    })
                                    .children(screen.todo.iter().enumerate().map(
                                        |(index, row)| {
                                            let focused = cursor == Some(index);
                                            let open =
                                                menu_open.as_deref() == Some(row.id.as_str());
                                            self.render_todo_row(
                                                row,
                                                focused,
                                                open,
                                                &options,
                                                areas.clone(),
                                                feedback,
                                                cx,
                                            )
                                        },
                                    )),
                            )
                        })
                    })
                    .when(tab == IdeasTab::Assigned, |list| {
                        list.when(screen.assigned.is_empty(), |list| {
                            list.child(tab_empty(
                                theme,
                                ui::i18n::text("drawers.no_assigned"),
                                ui::i18n::text("drawers.assign_idea_hint"),
                            ))
                        })
                        .when(!screen.assigned.is_empty(), |list| {
                            list.child(clear_row(
                                theme,
                                "desktop-ideas-assigned-clear",
                                ui::i18n::text("drawers.assigned_ideas"),
                                feedback,
                                cx.listener(|app, _, _, cx| {
                                    cx.stop_propagation();
                                    app.clear_assigned_ideas(cx);
                                }),
                            ))
                            .children(
                                screen.assigned.iter().enumerate().map(|(index, row)| {
                                    let focused = cursor == Some(index);
                                    self.render_assigned_row(row, focused, feedback, cx)
                                }),
                            )
                        })
                    })
                    .when(tab == IdeasTab::Done, |list| {
                        list.when(screen.done.is_empty(), |list| {
                            list.child(tab_empty(
                                theme,
                                ui::i18n::text("drawers.no_completed"),
                                ui::i18n::text("drawers.completed_hint"),
                            ))
                        })
                        .when(!screen.done.is_empty(), |list| {
                            list.child(clear_row(
                                theme,
                                "desktop-ideas-done-clear",
                                ui::i18n::text("drawers.done_ideas"),
                                feedback,
                                cx.listener(|app, _, _, cx| {
                                    cx.stop_propagation();
                                    app.clear_done_ideas(cx);
                                }),
                            ))
                            .children(screen.done.iter().map(|row| {
                                div()
                                    .id(ElementId::Name(
                                        format!("desktop-idea-done-{}", row.id).into(),
                                    ))
                                    .min_h(px(36.0))
                                    .px(px(12.0))
                                    .flex()
                                    .items_center()
                                    .gap(px(10.0))
                                    .text_size(px(13.0))
                                    .text_color(rgba(theme.text_tertiary))
                                    .child(
                                        div()
                                            .flex_none()
                                            .text_size(px(12.0))
                                            .text_color(rgba(ui::icons::with_alpha(
                                                theme.success,
                                                0xcc,
                                            )))
                                            .child("✓"),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .child(row.title.clone()),
                                    )
                            }))
                        })
                    }),
            )
            .children(self.render_pending_idea_images(cx))
            .when_some(self.desktop_workspace.ideas_input.clone(), |body, input| {
                let focused = self.desktop_workspace.ideas_aux.input_focused;
                let has_draft = !input.read(cx).value().trim().is_empty()
                    || !self.desktop_workspace.ideas_aux.pending_images.is_empty();
                body.child(
                    div()
                        .id("desktop-ideas-add")
                        .debug_selector(|| "desktop-ideas-add".to_string())
                        // ⇧回车换行（普通回车由输入框发出 PressEnter 来添加）。
                        .capture_key_down(cx.listener(|app, event: &KeyDownEvent, window, cx| {
                            let modifiers = &event.keystroke.modifiers;
                            if event.keystroke.key == "enter"
                                && modifiers.shift
                                && !modifiers.platform
                                && !modifiers.control
                                && !modifiers.alt
                            {
                                app.insert_idea_newline(window, cx);
                                cx.stop_propagation();
                            }
                        }))
                        // 剪贴板只有图片时由想法栏接管粘贴；有文字时交还输入框。
                        .capture_action(cx.listener(|app, _: &Paste, _, cx| {
                            if app.paste_idea_image(cx) {
                                cx.stop_propagation();
                            }
                        }))
                        .flex_none()
                        .mx(px(8.0))
                        .mt(px(6.0))
                        .child(
                            // 输入面：焦点时描边加亮加粗（Material filled field 的 active indicator）。
                            div()
                                .flex()
                                .items_end()
                                .gap(px(6.0))
                                .py(px(6.0))
                                .pl(px(16.0))
                                .pr(px(6.0))
                                .rounded(px(22.0))
                                .border_1()
                                .border_color(rgba(if focused {
                                    theme.focused_stroke
                                } else {
                                    theme.node_stroke
                                }))
                                .bg(rgba(if focused {
                                    theme.node_fill
                                } else {
                                    theme.panel_high
                                }))
                                .hover(|style| style.bg(rgba(theme.node_fill)))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .min_h(px(32.0))
                                        .flex()
                                        .items_center()
                                        .text_size(px(13.0))
                                        .text_color(rgba(theme.text_primary))
                                        .child(Input::new(&input).xsmall().appearance(false)),
                                )
                                .child(idea_send_button(theme, has_draft).when(
                                    has_draft,
                                    |button| {
                                        button.on_click(cx.listener(|app, _, window, cx| {
                                            cx.stop_propagation();
                                            app.add_idea_from_input(window, cx);
                                        }))
                                    },
                                )),
                        )
                        .when(focused || has_draft, |composer| {
                            composer.child(
                                div()
                                    .h(px(22.0))
                                    .px(px(14.0))
                                    .flex()
                                    .items_center()
                                    .text_size(px(11.0))
                                    .text_color(rgba(theme.text_tertiary))
                                    .child(ui::i18n::text("drawers.compose_hint")),
                            )
                        }),
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
                            .bg(rgba(ui::icons::with_alpha(theme.accent_warm, 0x26)))
                            .text_size(px(12.0))
                            .text_color(rgba(theme.accent_warm))
                            .child(notice),
                    )
                },
            )
            .when_some(
                self.desktop_workspace.ideas_undo.last().cloned(),
                |body, undo| {
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
                            .bg(rgba(ui::icons::with_alpha(theme.accent, 0x1f)))
                            .text_size(px(12.0))
                            .text_color(rgba(theme.text_primary))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .overflow_hidden()
                                    .child(undo.message()),
                            )
                            .child(
                                Button::new("desktop-ideas-undo-button")
                                    .ghost()
                                    .child(ui::i18n::text("drawers.undo"))
                                    .on_click(cx.listener(|app, _, _, cx| {
                                        cx.stop_propagation();
                                        app.undo_last_idea_action(cx);
                                    })),
                            ),
                    )
                },
            )
    }
}
