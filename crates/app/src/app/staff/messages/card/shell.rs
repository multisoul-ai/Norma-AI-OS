use super::super::*;
use gpui::{rgba, Div};

pub(super) fn card(ctx: &BlockCtx, emphasis: bool) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(tokens::SPACE_12))
        .px(px(if emphasis {
            tokens::EMPHASIS_RADIUS
        } else {
            tokens::SPACE_16
        }))
        .py(px(if emphasis {
            tokens::SPACE_16
        } else {
            tokens::SPACE_12
        }))
        .rounded(px(if emphasis {
            tokens::EMPHASIS_RADIUS
        } else {
            tokens::SPACE_16
        }))
        .bg(rgba(ctx.app.staff_theme().panel_high))
}
pub(super) fn header() -> Div {
    div().flex().items_center().gap(px(tokens::SPACE_8))
}

pub(super) fn action_button(
    ctx: &BlockCtx,
    id: impl Into<gpui::ElementId>,
    primary: bool,
    cx: &gpui::App,
) -> ui::component::button::Button {
    use ui::component::{
        button::{Button, ButtonVariants},
        Sizable,
    };
    Button::new(id)
        .custom(if primary {
            ctx.app.staff_primary_style(cx)
        } else {
            ctx.app.staff_button_style(false, cx)
        })
        .small()
        .compact()
        .h(px(tokens::BUTTON_HEIGHT))
        .px(px(tokens::SPACE_16))
        .rounded_full()
        .text_size(px(tokens::LABEL_TEXT))
}
