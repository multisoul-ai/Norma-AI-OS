use super::*;

pub(super) fn render(
    ctx: &BlockCtx,
    message: &TaskMessage,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    match message {
        TaskMessage::User { .. } => text::user::User(message).render(ctx, window, cx),
        TaskMessage::Staff(reply) => reply::Reply(reply).render(ctx, window, cx),
        TaskMessage::WorkerQuestion { question_id } => match ctx.task.question(question_id) {
            Some(question) => card::question::Question(question).render(ctx, window, cx),
            None => div().into_any_element(),
        },
        TaskMessage::WorkerFinished { .. } => {
            card::worker_result::WorkerResult(message).render(ctx, window, cx)
        }
        TaskMessage::WorkerNeedsAttention { .. }
        | TaskMessage::WorkerStarted { .. }
        | TaskMessage::Reopened => card::event::Event(message).render(ctx, window, cx),
        // 答案在问题卡上原地显示，这条只是状态标记，行排序已经跳过它。
        TaskMessage::Answered { .. } => div().into_any_element(),
    }
}
