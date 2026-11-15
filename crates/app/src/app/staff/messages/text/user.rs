use super::super::*;
use gpui::{img, rgba, ObjectFit};
use std::path::PathBuf;

pub(in crate::app::staff::messages) struct User<'a>(pub &'a TaskMessage);

impl MessageBlock for User<'_> {
    fn render(
        &self,
        ctx: &BlockCtx,
        _window: &mut Window,
        _cx: &mut Context<MultisoulApp>,
    ) -> AnyElement {
        let index = ctx.index;
        let theme = ctx.app.staff_theme();
        let message = self.0;
        let mut row = div()
            .w_full()
            // Intrinsic height must use the same reading width as the final message column.
            .max_w(px(tokens::READING_WIDTH))
            .flex()
            .flex_col()
            .gap(px(tokens::SPACE_8));
        if let TaskMessage::User { text } = message {
            let (visible, paths) = norma::protocol::image_message_parts(text);
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
                        .child(visible),
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
            row = row.items_end().child(bubble);
        }
        row.into_any_element()
    }
}
