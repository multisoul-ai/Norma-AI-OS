use super::super::*;
use ui::component::text::TextView;

pub(in crate::app::staff::messages) struct Prose<'a>(pub &'a TaskMessage);

impl MessageBlock for Prose<'_> {
    fn render(
        &self,
        ctx: &BlockCtx,
        window: &mut Window,
        cx: &mut Context<MultisoulApp>,
    ) -> AnyElement {
        let task = ctx.task;
        let index = ctx.index;
        let message = self.0;
        let mut row = div().w_full().flex().flex_col().gap(px(tokens::SPACE_8));
        // Keep Markdown height intrinsic instead of resolving its nested percentages against flex stretch.
        row.style().align_self = Some(gpui::AlignSelf::FlexStart);
        if let TaskMessage::Norma { text } = message {
            row = row.child(super::staff_byline(ctx.app)).child(
                TextView::markdown(
                    gpui::SharedString::from(format!("staff-markdown-{}-{index}", task.id)),
                    text.clone(),
                    window,
                    cx,
                )
                .selectable(true)
                // TextView defaults to `size_full()`; a percentage height inside the auto-height message
                // stack must not decide how much room the wrapped text gets, or the next block rides up.
                .h_auto()
                .flex_none(),
            );
        }
        row.into_any_element()
    }
}
