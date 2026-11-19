pub(super) mod user;

use super::tokens;
use gpui::{div, img, linear_color_stop, linear_gradient, prelude::*, px, rgba, Div};
use ui::component::StyledExt;

/// Norma 署名：与桌面首页一致的渐变描边头像、在线点和紫色名字。
pub(in crate::app::staff::messages) fn staff_byline(app: &crate::app::MultisoulApp) -> Div {
    let theme = app.staff_theme();
    let light = app.theme.is_light();
    div()
        .flex()
        .items_center()
        .gap(px(tokens::SPACE_8))
        .text_size(px(tokens::LABEL_TEXT))
        .child(
            div()
                .relative()
                .flex_none()
                .size(px(36.))
                .rounded_full()
                .p(px(2.))
                .bg(linear_gradient(
                    140.,
                    linear_color_stop(rgba(0x8e7cf8ff), 0.),
                    linear_color_stop(rgba(0x6fd3e8ff), 1.),
                ))
                .child(
                    img(crate::assets::NORMA_CALM_ASSET)
                        .size_full()
                        .rounded_full()
                        .object_fit(gpui::ObjectFit::Cover),
                )
                .child(
                    div()
                        .absolute()
                        .right(px(-1.))
                        .bottom(px(-1.))
                        .size(px(11.))
                        .rounded_full()
                        .border_2()
                        .border_color(rgba(theme.graphite))
                        .bg(rgba(theme.success)),
                ),
        )
        .child(
            div()
                .font_medium()
                .text_color(rgba(if light { 0x5b4bd6ff } else { 0xb9b0faff }))
                .child("Norma"),
        )
        .child(
            div()
                .text_size(px(tokens::META_TEXT))
                .text_color(rgba(theme.text_secondary))
                .child(ui::i18n::text("staff.coordination")),
        )
}
