use super::super::*;
use super::shell;
use gpui::rgba;
use norma::protocol::WorkerOutcome;
use runtime::island::bloub::Mood;
use ui::i18n::text as t;

/// 折叠时显示的摘要行数。
const SUMMARY_LINES: usize = 3;
/// 摘要的固定行高；折叠高度 = 行数 × 行高，折行的长段落也只占 3 行。
const SUMMARY_LINE_HEIGHT: f32 = 20.;
/// 卡片一行大约能放的字数（按中文宽度保守估计），用来估算折行后的显示行数。
const CHARS_PER_LINE: usize = 40;

/// 按段落估算折行后的显示行数，空行占一行。
fn display_lines(body: &str) -> usize {
    body.lines()
        .map(|line| line.chars().count().div_ceil(CHARS_PER_LINE).max(1))
        .sum()
}

pub(in crate::app::staff::messages) struct WorkerResult<'a>(pub &'a TaskMessage);

impl MessageBlock for WorkerResult<'_> {
    fn render(
        &self,
        ctx: &BlockCtx,
        _window: &mut Window,
        cx: &mut Context<MultisoulApp>,
    ) -> AnyElement {
        let task = ctx.task;
        let theme = ctx.app.staff_theme();
        let message = self.0;
        let mut row = div().w_full().flex().flex_col().gap(px(tokens::SPACE_8));
        if let TaskMessage::WorkerFinished {
            node_id,
            outcome,
            text,
            ..
        } = message
        {
            let worker = task
                .workers
                .iter()
                .find(|worker| worker.card_id == Some(*node_id));
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
                    let body = if matches!(
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
                    // 默认只显示 3 行（按显示行数截断，长段落折行后也不会顶到按钮），长结果可以展开。
                    let key = format!("worker-result-{}-{}", task.id, ctx.index);
                    let long = display_lines(&body) > SUMMARY_LINES;
                    let open = ctx.app.staff.expanded.contains(&key);
                    let index = ctx.index;
                    row.child(
                        div()
                            .text_size(px(tokens::LABEL_TEXT))
                            .line_height(px(SUMMARY_LINE_HEIGHT))
                            .when(long && !open, |text| {
                                text.max_h(px(SUMMARY_LINE_HEIGHT * SUMMARY_LINES as f32))
                                    .overflow_hidden()
                            })
                            .child(body),
                    )
                    .when(long, |row| {
                        // 放进横向容器里，按钮只占内容宽度，不再撑满整行。
                        row.child(
                            div().flex().child(
                                shell::action_button(
                                    ctx,
                                    ("staff-result-toggle", index),
                                    false,
                                    cx,
                                )
                                .debug_selector(move || format!("staff-result-toggle-{index}"))
                                .label(t(if open {
                                    "staff.result_collapse"
                                } else {
                                    "staff.result_expand"
                                }))
                                .on_click(cx.listener(
                                    move |app, _, _, cx| {
                                        if !app.staff.expanded.remove(&key) {
                                            app.staff.expanded.insert(key.clone());
                                        }
                                        app.notify_staff(cx);
                                    },
                                )),
                            ),
                        )
                    })
                });
        }
        row.into_any_element()
    }
}

#[cfg(test)]
#[path = "../../../../../tests/unit/staff_result_summary_tests.rs"]
mod tests;
