use super::super::*;
use gpui::rgba;

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
        let mut row = div().w_full().flex().flex_col().gap(px(tokens::SPACE_8));
        if let TaskMessage::User { text } = message {
            row = row.flex_row().justify_end().child(
                div()
                    .debug_selector(move || format!("staff-user-bubble-{index}"))
                    .max_w(relative(0.78))
                    .px(px(tokens::SPACE_16))
                    .py(px(tokens::SPACE_12))
                    .rounded(px(tokens::BUBBLE_RADIUS))
                    .rounded_br(px(tokens::SPACE_4))
                    .bg(rgba(theme.panel_high))
                    .child(text.clone()),
            );
        }
        row.into_any_element()
    }
}
