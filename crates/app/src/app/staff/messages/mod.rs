use super::view::outcome_label;
use super::*;
use gpui::{div, prelude::*, px, relative, AnyElement, Window};
use norma::protocol::TaskMessage;
mod card;
mod registry;
mod text;
pub(super) mod tokens;

struct BlockCtx<'a> {
    app: &'a MultisoulApp,
    task: &'a Task,
    index: usize,
}
trait MessageBlock {
    fn render(
        &self,
        ctx: &BlockCtx,
        window: &mut Window,
        cx: &mut Context<MultisoulApp>,
    ) -> AnyElement;
}
impl MultisoulApp {
    pub(super) fn render_message(
        &self,
        task: &Task,
        index: usize,
        message: &TaskMessage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ctx = BlockCtx {
            app: self,
            task,
            index,
        };
        message_column()
            .debug_selector(move || format!("staff-message-{index}"))
            .child(
                div()
                    .id(("staff-message", index))
                    .debug_selector(move || format!("staff-message-body-{index}"))
                    .w_full()
                    .flex()
                    .flex_none()
                    .text_size(px(tokens::BODY_TEXT))
                    .line_height(relative(tokens::BODY_LEADING))
                    .child(registry::render(&ctx, message, window, cx)),
            )
            .into_any_element()
    }
}
pub(super) fn message_column() -> gpui::Div {
    div()
        .flex_none()
        .flex()
        .w_full()
        .max_w(px(tokens::READING_WIDTH))
        .mx_auto()
}
