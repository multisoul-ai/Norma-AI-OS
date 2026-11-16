use super::super::*;
use gpui::{img, rgba, ObjectFit};
use std::path::PathBuf;

pub(in crate::app::staff::messages) struct User<'a>(pub &'a TaskMessage);

impl MessageBlock for User<'_> {
    fn render(
        &self,
        ctx: &BlockCtx,
        window: &mut Window,
        cx: &mut Context<MultisoulApp>,
    ) -> AnyElement {
        let index = ctx.index;
        let theme = ctx.app.staff_theme();
        let message = self.0;
        // 块级行先拿到列宽，再让横向 flex 在这个确定宽度里把气泡收到 78% 并靠右。
        // 直接用纵向 flex 时，百分比宽度在最小内容测量里不生效，中文会一字一行把行高撑开。
        let mut row = div()
            .debug_selector(move || format!("staff-user-row-{index}"))
            .w_full()
            .max_w(px(tokens::READING_WIDTH))
            .block()
            .flex_none();
        if let TaskMessage::User { text } = message {
            let (visible, paths) = norma::protocol::image_message_parts(text);
            let markdown = visible.contains("http://") || visible.contains("https://");
            let mut bubble = div()
                .flex_none()
                .min_w_0()
                .debug_selector(move || format!("staff-user-bubble-{index}"))
                .max_w(relative(0.78))
                .px(px(tokens::SPACE_16))
                .py(px(tokens::SPACE_12))
                .rounded(px(tokens::BUBBLE_RADIUS))
                .rounded_br(px(tokens::SPACE_4))
                .bg(rgba(theme.panel_high));
            if !visible.is_empty() {
                bubble = bubble.child(
                    div()
                        .debug_selector(move || format!("staff-user-text-{index}"))
                        .when(!paths.is_empty(), |text| text.mb(px(tokens::SPACE_8)))
                        .child(if markdown {
                            crate::app::staff::register_link_target(window, cx);
                            ui::component::text::TextView::markdown(
                                gpui::SharedString::from(format!("staff-user-md-{index}")),
                                crate::preview::rich_link::linkify_plain_text(&visible),
                                window,
                                cx,
                            )
                            .selectable(true)
                            .h_auto()
                            .flex_none()
                            .into_any_element()
                        } else {
                            visible.into_any_element()
                        }),
                );
            }
            let image_count = paths.len();
            for (image_index, path) in paths.into_iter().enumerate() {
                bubble = bubble.child(
                    div()
                        .id(gpui::ElementId::named_usize(
                            format!("staff-user-image-{index}"),
                            image_index,
                        ))
                        .debug_selector(move || format!("staff-user-image-{index}-{image_index}"))
                        .w(px(220.))
                        .max_w_full()
                        .h(px(160.))
                        .flex_none()
                        .when(image_index + 1 < image_count, |image| {
                            image.mb(px(tokens::SPACE_8))
                        })
                        .rounded(px(12.))
                        .child(
                            img(PathBuf::from(path))
                                .debug_selector(move || {
                                    format!("staff-user-image-content-{index}-{image_index}")
                                })
                                .w_full()
                                .h(px(160.))
                                .max_h(px(160.))
                                .rounded(px(12.))
                                .object_fit(ObjectFit::Contain)
                                .with_fallback(|| {
                                    div()
                                        .size_full()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(ui::i18n::text("staff.image_unavailable"))
                                        .into_any_element()
                                }),
                        ),
                );
            }
            row = row.child(div().w_full().flex().justify_end().child(bubble));
        }
        row.into_any_element()
    }
}
