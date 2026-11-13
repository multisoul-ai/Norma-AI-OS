use super::*;

pub(super) fn render(
    ctx: &BlockCtx,
    message: &TaskMessage,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    match message {
        TaskMessage::Question { question } => div()
            .children(
                question
                    .items
                    .iter()
                    .map(|item| div().child(item.prompt.clone())),
            )
            .into_any_element(),
        TaskMessage::User { .. } => text::user::User(message).render(ctx, window, cx),
        TaskMessage::Norma { .. } => text::prose::Prose(message).render(ctx, window, cx),
        TaskMessage::Proposal { .. } => card::proposal::Proposal(message).render(ctx, window, cx),
        TaskMessage::WorkerFinished { .. } => {
            card::worker_result::WorkerResult(message).render(ctx, window, cx)
        }
        TaskMessage::WorkerNeedsAttention { .. } => {
            card::needs_attention::NeedsAttention(message).render(ctx, window, cx)
        }
        TaskMessage::WorkerStarted { .. } | TaskMessage::Reopened => {
            text::notice::Notice(message).render(ctx, window, cx)
        }
    }
}
