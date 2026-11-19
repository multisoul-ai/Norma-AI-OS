use super::view::outcome_label;
use super::*;
use gpui::{div, prelude::*, px, relative, AnyElement, Window};
use norma::protocol::TaskMessage;
mod card;
mod registry;
mod reply;
pub(super) mod rows;
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
        message_row(
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
                ),
        )
        .into_any_element()
    }
}
/// 阅读列：宽度一开始就是确定的 760，窄窗时收缩。
///
/// 不能写成纵向列表里的 `w_full().max_w(760)`：Taffy 量这一行的高度时按拉伸后的整行宽度排版，
/// 宽窗下行数偏少；最后再把宽度夹回 760，多出来的行画到盒子外面，下一条从偏矮的底边排起，
/// 压住上一条的最后几行。`flex_1().max_w(760)` 同理。
pub(super) fn message_column() -> gpui::Div {
    div()
        .w(px(tokens::READING_WIDTH))
        .flex_shrink()
        .min_w_0()
        .flex()
}

/// 时间线里的一行：把阅读列放在横向主轴上居中。
pub(super) fn message_row(column: gpui::Div) -> gpui::Div {
    div()
        .flex_none()
        .w_full()
        .flex()
        .justify_center()
        .child(column)
}
