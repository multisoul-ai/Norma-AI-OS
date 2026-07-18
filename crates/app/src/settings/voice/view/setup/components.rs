use gpui::{div, prelude::*, px, rgba, Div, FontWeight, Stateful};
use ui::Theme;

pub(super) fn compact_setup_row(
    theme: Theme,
    title: &'static str,
    hint: &'static str,
    action: Stateful<Div>,
) -> Div {
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(16.0))
        .py(px(4.0))
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
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgba(theme.text_primary))
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgba(theme.text_tertiary))
                        .child(hint),
                ),
        )
        .child(action.flex_none())
}
