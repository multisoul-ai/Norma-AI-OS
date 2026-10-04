use super::*;
use gpui::{div, prelude::*, px, rgba, Focusable, MouseButton, Render};
use ui::component::StyledExt;
use ui::component::{
    button::{Button, ButtonVariants},
    input::Input,
    switch::Switch,
    Disableable, Selectable, Sizable,
};
use ui::icons::with_alpha;

pub(super) fn editor(
    id: &'static str,
    input: &Entity<InputState>,
    disabled: bool,
    height: Option<f32>,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let focus = input.focus_handle(cx);
    let field = Input::new(input).disabled(disabled).small();
    let field = if let Some(height) = height {
        field.h(px(height))
    } else {
        field
    };
    div()
        .id(id)
        .w_full()
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            if !disabled {
                focus.focus(window);
            }
            cx.stop_propagation();
        })
        .child(field)
}

impl Render for MemoryView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .id("norma-memory")
            .debug_selector(|| "norma-memory".into())
            .size_full()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .gap_3()
            .text_color(rgba(theme.text_primary))
            .text_size(px(13.0))
            .child(
                div()
                    .flex()
                    .gap_6()
                    .flex_none()
                    .p_4()
                    .rounded(px(12.0))
                    .bg(rgba(theme.panel_high))
                    .child(self.preference(true, cx))
                    .child(self.preference(false, cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .flex_none()
                    .gap_3()
                    .child(
                        div().flex().gap_1().children(
                            [
                                MemoryScopeAlias::Personal,
                                MemoryScopeAlias::Project,
                                MemoryScopeAlias::Worktree,
                            ]
                            .into_iter()
                            .map(|scope| {
                                Button::new(gpui::ElementId::Name(
                                    format!("memory-scope-{scope:?}").into(),
                                ))
                                .small()
                                .ghost()
                                .label(format!(
                                    "{}{}",
                                    scope.label(),
                                    if self.drafts.contains_key(&scope) {
                                        " · 草稿"
                                    } else {
                                        ""
                                    }
                                ))
                                .selected(self.scope == scope)
                                .disabled(
                                    self.busy
                                        || self.pending_save.is_some()
                                        || (self.root.is_none()
                                            && scope != MemoryScopeAlias::Personal),
                                )
                                .on_click(cx.listener(
                                    move |view, _, window, cx| view.change_scope(scope, window, cx),
                                ))
                            }),
                        ),
                    )
                    .child(
                        Button::new("memory-reload")
                            .small()
                            .ghost()
                            .label(if self.busy { "载入中…" } else { "刷新" })
                            .disabled(self.busy)
                            .on_click(cx.listener(|view, _, window, cx| view.reload(window, cx))),
                    ),
            )
            .when(self.root.is_none(), |content| {
                content.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgba(theme.text_secondary))
                        .child("个人记忆随你使用；打开项目后可管理项目与工作树记忆。"),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .gap_0()
                    .border_1()
                    .border_color(rgba(with_alpha(theme.text_secondary, 0x26)))
                    .rounded(px(10.0))
                    .child(self.library(cx))
                    .child(self.detail(cx)),
            )
            .when(!self.status.is_empty(), |content| {
                content.child(
                    div()
                        .flex_none()
                        .text_size(px(11.0))
                        .text_color(rgba(if self.error {
                            theme.destructive
                        } else {
                            theme.text_secondary
                        }))
                        .child(self.status.clone()),
                )
            })
            .when(self.close_prompt, |content| {
                content.child(
                    div()
                        .flex_none()
                        .p_3()
                        .rounded(px(8.0))
                        .bg(rgba(with_alpha(theme.accent_orange, 0x12)))
                        .child(div().text_size(px(12.0)).child(
                            "有未保存的修改。返回标有“草稿”的范围保存，或选择放弃全部草稿。",
                        ))
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .mt_2()
                                .child(
                                    Button::new("memory-discard-all")
                                        .small()
                                        .label("放弃全部草稿")
                                        .disabled(self.busy || self.pending_save.is_some())
                                        .on_click(cx.listener(|view, _, window, cx| {
                                            view.drafts.clear();
                                            view.discard(window, cx);
                                        })),
                                )
                                .child(
                                    Button::new("memory-continue-all")
                                        .small()
                                        .ghost()
                                        .label("继续编辑")
                                        .on_click(cx.listener(|view, _, _, cx| {
                                            view.close_prompt = false;
                                            cx.notify();
                                        })),
                                ),
                        ),
                )
            })
    }
}

impl MemoryView {
    fn preference(&self, recall: bool, cx: &mut Context<Self>) -> gpui::Div {
        let (label, description, checked) = if recall {
            ("使用记忆", "交流时按需读取", self.preferences.recall)
        } else {
            ("保存记忆", "你明确说“记住”时保存", self.preferences.save)
        };
        div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().font_medium().child(label))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgba(self.theme.text_secondary))
                            .child(description),
                    ),
            )
            .child(
                Switch::new(if recall {
                    "norma-memory-recall"
                } else {
                    "norma-memory-save"
                })
                .small()
                .checked(checked)
                .disabled(self.busy)
                .on_click(cx.listener(move |view, value, _, cx| view.toggle(recall, *value, cx))),
            )
    }
}
