use super::*;
use gpui::{div, prelude::*, px, rgba, Render, Window};
use ui::component::StyledExt;
use ui::component::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    Disableable, Sizable,
};
use ui::icons::with_alpha;

fn caption(theme: ui::Theme, text: impl Into<gpui::SharedString>) -> gpui::Div {
    div()
        .text_size(px(12.0))
        .line_height(px(20.0))
        .text_color(rgba(theme.text_secondary))
        .child(text.into())
}

impl Render for PersonaView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let draft = self.draft(cx);
        let validation = draft.validate().err();
        let disabled = self.busy || !self.loaded;
        div()
            .id("norma-persona")
            .debug_selector(|| "norma-persona".into())
            .w_full()
            .max_w(px(780.0))
            .flex()
            .flex_col()
            .gap_4()
            .text_color(rgba(theme.text_primary))
            .text_size(px(14.0))
            .when(self.close_prompt, |page| {
                page.child(self.unsaved_prompt(cx))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .size(px(52.0))
                            .flex_none()
                            .rounded(px(16.0))
                            .bg(rgba(with_alpha(theme.accent, 0x16)))
                            .text_color(rgba(theme.accent))
                            .text_size(px(25.0))
                            .font_semibold()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child("N"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().font_semibold().text_size(px(18.0)).child("Norma"))
                            .child(caption(
                                theme,
                                "你的 AI 伙伴 · 与你交流，帮你协调项目与智能体",
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(div().font_semibold().child("交流风格"))
                    .child(caption(
                        theme,
                        "选择你喜欢的表达方式。所有个人对话与项目会话共用这份人设。",
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_3()
                            .child(self.tone_card(PersonaTone::Warm, disabled, cx))
                            .child(self.tone_card(PersonaTone::Professional, disabled, cx))
                            .child(self.tone_card(PersonaTone::Concise, disabled, cx)),
                    ),
            )
            .child(
                div()
                    .id("persona-example")
                    .debug_selector(|| "persona-example".into())
                    .p_4()
                    .rounded(px(12.0))
                    .bg(rgba(with_alpha(theme.accent, 0x0c)))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(caption(theme, "表达示例"))
                    .child(div().line_height(px(23.0)).child(self.tone.example())),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().font_semibold().child("补充说明"))
                    .child(caption(
                        theme,
                        "告诉 Norma 你希望如何交流，例如语言、解释深度或称呼习惯。",
                    ))
                    .child(crate::settings::norma::view::editor(
                        "persona-instructions",
                        &self.instructions,
                        disabled,
                        Some(110.0),
                        cx,
                    ))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .gap_3()
                            .child(caption(
                                theme,
                                "人设影响交流方式，记忆由「记忆」页面独立管理。",
                            ))
                            .child(caption(
                                theme,
                                format!(
                                    "{} / {}",
                                    draft.instructions.chars().count(),
                                    agent::MAX_PERSONA_INSTRUCTION_CHARS
                                ),
                            )),
                    ),
            )
            .when(validation.is_some(), |page| {
                page.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgba(theme.destructive))
                        .child(validation.unwrap_or_default()),
                )
            })
            .when(!self.status.is_empty(), |page| {
                page.child(
                    div()
                        .id("persona-status")
                        .debug_selector(|| "persona-status".into())
                        .text_size(px(12.0))
                        .text_color(rgba(if self.error {
                            theme.destructive
                        } else {
                            theme.text_secondary
                        }))
                        .child(self.status.clone()),
                )
            })
            .when(!self.loaded && !self.busy, |page| {
                page.child(
                    Button::new("persona-reload")
                        .small()
                        .label("重新载入")
                        .on_click(cx.listener(|view, _, window, cx| view.reload(window, cx))),
                )
            })
            .child(
                div()
                    .id("persona-actions")
                    .debug_selector(|| "persona-actions".into())
                    .flex()
                    .items_center()
                    .justify_between()
                    .flex_wrap()
                    .gap_3()
                    .pt_4()
                    .border_t_1()
                    .border_color(rgba(with_alpha(theme.text_secondary, 0x20)))
                    .child(
                        Button::new("persona-reset")
                            .ghost()
                            .small()
                            .label("恢复默认")
                            .disabled(disabled)
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.replace(NormaPersona::default(), window, cx);
                                view.status = "默认人设已填入，保存后生效".into();
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(caption(
                                theme,
                                if draft != self.saved {
                                    "有未保存的修改"
                                } else {
                                    "已保存"
                                },
                            ))
                            .child(
                                Button::new("persona-cancel")
                                    .ghost()
                                    .small()
                                    .label("取消")
                                    .disabled(disabled || draft == self.saved)
                                    .on_click(
                                        cx.listener(|view, _, window, cx| view.discard(window, cx)),
                                    ),
                            )
                            .child(
                                Button::new("persona-save")
                                    .primary()
                                    .small()
                                    .label(if self.busy && self.loaded {
                                        "保存中…"
                                    } else {
                                        "保存"
                                    })
                                    .disabled(
                                        disabled
                                            || draft == self.saved
                                            || draft.validate().is_err(),
                                    )
                                    .on_click(cx.listener(|view, _, _, cx| view.save(cx))),
                            ),
                    ),
            )
    }
}

impl PersonaView {
    fn unsaved_prompt(&self, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        div()
            .id("persona-unsaved-prompt")
            .debug_selector(|| "persona-unsaved-prompt".into())
            .p_4()
            .rounded(px(12.0))
            .bg(rgba(with_alpha(self.theme.accent_orange, 0x12)))
            .child(div().text_size(px(13.0)).child("人设还有未保存的修改。"))
            .child(
                div()
                    .mt_3()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("persona-discard")
                            .small()
                            .label("放弃修改")
                            .disabled(self.busy)
                            .on_click(cx.listener(|view, _, window, cx| view.discard(window, cx))),
                    )
                    .child(
                        Button::new("persona-continue")
                            .ghost()
                            .small()
                            .label("继续编辑")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.close_prompt = false;
                                cx.notify();
                            })),
                    ),
            )
    }

    fn tone_card(
        &self,
        tone: PersonaTone,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let selected = self.tone == tone;
        let theme = self.theme;
        let variant = ButtonCustomVariant::new(cx)
            .color(
                rgba(if selected {
                    with_alpha(theme.accent, 0x14)
                } else {
                    theme.panel_high
                })
                .into(),
            )
            .foreground(rgba(theme.text_primary).into())
            .border(
                rgba(if selected {
                    with_alpha(theme.accent, 0xa0)
                } else {
                    with_alpha(theme.text_secondary, 0x25)
                })
                .into(),
            )
            .hover(rgba(with_alpha(theme.accent, 0x22)).into())
            .active(rgba(with_alpha(theme.accent, 0x35)).into());
        div()
            .id(gpui::ElementId::Name(
                format!("persona-tone-{tone:?}").into(),
            ))
            .debug_selector(move || format!("persona-tone-{tone:?}"))
            .flex_1()
            .min_w(px(160.0))
            .child(
                Button::new(gpui::ElementId::Name(
                    format!("persona-tone-button-{tone:?}").into(),
                ))
                .custom(variant)
                .w_full()
                .h(px(76.0))
                .justify_start()
                .disabled(disabled)
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(div().font_semibold().child(tone.label()))
                                .child(div().text_color(rgba(theme.accent)).child(if selected {
                                    "✓"
                                } else {
                                    ""
                                })),
                        )
                        .child(caption(theme, tone.description())),
                )
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.tone = tone;
                    view.error = false;
                    view.status.clear();
                    cx.notify();
                })),
            )
    }
}
