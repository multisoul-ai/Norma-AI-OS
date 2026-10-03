use super::*;
use gpui::{div, prelude::*, px, rgba};
use ui::component::StyledExt;
use ui::component::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    Disableable, Sizable,
};
use ui::icons::with_alpha;

impl MemoryView {
    pub(super) fn library(&self, cx: &mut Context<Self>) -> gpui::Div {
        let theme = self.theme;
        let query = self.search.read(cx).value().to_lowercase();
        let filtered: Vec<_> = self
            .entries
            .iter()
            .filter(|entry| {
                self.search_texts
                    .get(entry.id.as_str())
                    .is_some_and(|text| text.contains(&query))
            })
            .collect();
        let mut list = div()
            .id("memory-topic-list")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1()
            .p_2();
        if filtered.is_empty() {
            list = list.child(
                div()
                    .p_4()
                    .text_size(px(12.0))
                    .text_color(rgba(theme.text_secondary))
                    .child(if query.is_empty() {
                        "还没有记忆。\n从一条偏好或项目约定开始。"
                    } else {
                        "没有匹配的记忆。\n试试其他关键词。"
                    }),
            );
        }
        for entry in filtered {
            let id = entry.id.as_str().to_string();
            let selected = self.selected.as_deref() == Some(&id);
            let variant = ButtonCustomVariant::new(cx)
                .color(
                    rgba(if selected {
                        with_alpha(theme.accent, 0x18)
                    } else {
                        with_alpha(theme.panel, 0)
                    })
                    .into(),
                )
                .foreground(rgba(theme.text_primary).into())
                .border(rgba(with_alpha(theme.accent, 0)).into())
                .hover(rgba(with_alpha(theme.accent, 0x20)).into())
                .active(rgba(with_alpha(theme.accent, 0x30)).into());
            list = list.child(
                Button::new(gpui::ElementId::Name(format!("memory-topic-{id}").into()))
                    .custom(variant)
                    .w_full()
                    .h(px(62.0))
                    .justify_start()
                    .disabled(self.busy)
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap_1()
                            .items_start()
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_medium()
                                    .text_ellipsis()
                                    .child(entry.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(rgba(theme.text_secondary))
                                    .text_ellipsis()
                                    .child(if entry.description.is_empty() {
                                        "长期记忆".into()
                                    } else {
                                        entry.description.clone()
                                    }),
                            ),
                    )
                    .on_click(
                        cx.listener(move |view, _, window, cx| view.select(id.clone(), window, cx)),
                    ),
            );
        }
        div()
            .w(px(235.0))
            .flex_none()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(rgba(with_alpha(theme.text_secondary, 0x26)))
            .rounded_tl(px(10.0))
            .rounded_bl(px(10.0))
            .bg(rgba(with_alpha(theme.panel, 0xb0)))
            .child(div().flex_none().p_3().child(super::view::editor(
                "memory-search-input",
                &self.search,
                self.busy,
                None,
                cx,
            )))
            .child(list)
            .child(
                div()
                    .flex_none()
                    .p_3()
                    .border_t_1()
                    .border_color(rgba(with_alpha(theme.text_secondary, 0x20)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                Button::new("memory-new")
                                    .small()
                                    .ghost()
                                    .label("＋ 新增")
                                    .disabled(self.busy)
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.new_entry(window, cx)
                                    })),
                            )
                            .child(
                                Button::new("memory-import")
                                    .small()
                                    .ghost()
                                    .label("导入…")
                                    .disabled(self.busy)
                                    .on_click(
                                        cx.listener(|view, _, window, cx| view.import(window, cx)),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .mt_2()
                            .text_size(px(10.0))
                            .text_color(rgba(theme.text_secondary))
                            .child(format!(
                                "{} 条记忆 · {}",
                                self.entries.len(),
                                self.scope.label()
                            )),
                    ),
            )
    }
}
