use super::super::*;
use super::shell;
use gpui::rgba;
use norma::protocol::WorkerOutcome;
use runtime::island::bloub::Mood;

/// 结果正文行高。高度由内容自己撑开，不再按字数估行后截断。
const BODY_LINE_HEIGHT: f32 = 20.;

pub(in crate::app::staff::messages) struct WorkerResult<'a>(pub &'a TaskMessage);

impl MessageBlock for WorkerResult<'_> {
    fn render(
        &self,
        ctx: &BlockCtx,
        window: &mut Window,
        cx: &mut Context<MultisoulApp>,
    ) -> AnyElement {
        let task = ctx.task;
        let theme = ctx.app.staff_theme();
        let message = self.0;
        let mut row = div().w_full().flex().flex_col().gap(px(tokens::SPACE_8));
        if let TaskMessage::WorkerFinished {
            node_id,
            dispatch_id,
            outcome,
            text,
        } = message
        {
            let worker = task
                .workers
                .iter()
                .find(|worker| worker.dispatch_id == *dispatch_id)
                .or_else(|| {
                    task.workers
                        .iter()
                        .find(|worker| worker.card_id == Some(*node_id))
                });
            let name = worker.map_or("Agent", |worker| worker.agent.as_str());
            let engine = worker.map_or("", |worker| worker.engine.as_str());
            row = shell::card(ctx, false)
                .child(
                    shell::header()
                        .text_size(px(tokens::SPACE_12))
                        .child(crate::island::pet(
                            name,
                            engine,
                            tokens::AVATAR_SIZE,
                            if *outcome == WorkerOutcome::Succeeded {
                                Mood::Responded
                            } else {
                                Mood::Failed
                            },
                            !ctx.app.motion.reduce_motion,
                        ))
                        .child(name.to_owned())
                        .child(
                            div()
                                .text_color(rgba(if *outcome == WorkerOutcome::Succeeded {
                                    theme.success
                                } else {
                                    theme.accent_warm
                                }))
                                .child(outcome_label(*outcome)),
                        ),
                )
                .when(*outcome != WorkerOutcome::Unavailable, |row| {
                    let body = if text == "staff.worker_withdrawn" {
                        ui::i18n::format(
                            "staff.worker_withdrawn",
                            &[
                                ("name", worker.map(|w| w.agent.clone()).unwrap_or_default()),
                                ("title", worker.map(|w| w.role.clone()).unwrap_or_default()),
                            ],
                        )
                    } else if matches!(
                        text.as_str(),
                        "staff.worker_failed"
                            | "staff.worker_cancelled"
                            | "staff.worker_not_ready"
                            | "staff.worker_needs_attention"
                    ) {
                        error_message(text.clone()).to_string()
                    } else {
                        text.clone()
                    };
                    let index = ctx.index;
                    crate::app::staff::register_link_target(window, cx);
                    row.child(
                        div()
                            .debug_selector(move || format!("staff-result-body-{index}"))
                            .w_full()
                            .flex_none()
                            .text_size(px(tokens::LABEL_TEXT))
                            .line_height(px(BODY_LINE_HEIGHT))
                            .child(
                                ui::component::text::TextView::markdown(
                                    gpui::SharedString::from(format!(
                                        "staff-result-md-{}-{index}",
                                        task.id
                                    )),
                                    crate::preview::rich_link::linkify_inline_paths(&body),
                                    window,
                                    cx,
                                )
                                .selectable(true)
                                .h_auto()
                                .flex_none(),
                            ),
                    )
                });
        }
        row.into_any_element()
    }
}
