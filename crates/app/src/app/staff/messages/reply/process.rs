//! 过程区：计划与步骤。运行中全部展开；结束后折叠成「已处理 N 步 · 用时」，可点开。
use super::step_title;
use crate::app::staff::messages::{tokens, BlockCtx};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, AnyElement, Context};
use norma::protocol::{PlanItem, ReplyStatus, StaffPart, StaffReply, StepStatus};
use ui::component::{
    button::{Button, ButtonVariants},
    spinner::Spinner,
    Disableable, Icon, IconName, Sizable,
};
use ui::i18n::text as t;

pub(super) fn render(
    ctx: &BlockCtx,
    reply: &StaffReply,
    cx: &mut Context<MultisoulApp>,
) -> Option<AnyElement> {
    let plan: Option<&Vec<PlanItem>> = reply.parts.iter().find_map(|part| match part {
        StaffPart::Plan(items) if !items.is_empty() => Some(items),
        _ => None,
    });
    let steps = reply.step_count();
    if steps == 0 && plan.is_none() {
        return None;
    }
    let app = ctx.app;
    let theme = app.staff_theme();
    let index = ctx.index;
    let running = reply.is_running();
    let key = format!("process-{}-{index}", ctx.task.id);
    let expanded = running || app.staff.expanded.contains(&key);
    let icon_size = px(tokens::BODY_TEXT);
    let lead = match reply.status {
        ReplyStatus::Running if app.motion.reduce_motion => Icon::new(IconName::Loader)
            .with_size(icon_size)
            .into_any_element(),
        ReplyStatus::Running => Spinner::new()
            .with_size(icon_size)
            .color(rgba(theme.accent_cyan).into())
            .into_any_element(),
        ReplyStatus::Done => Icon::new(IconName::Check)
            .with_size(icon_size)
            .text_color(rgba(theme.success))
            .into_any_element(),
        ReplyStatus::Stopped | ReplyStatus::Failed => Icon::new(IconName::TriangleAlert)
            .with_size(icon_size)
            .text_color(rgba(theme.accent_warm))
            .into_any_element(),
    };
    let seconds = format!("{:.1}", reply.elapsed_ms as f64 / 1000.);
    let summary = match (reply.status, steps, reply.elapsed_ms) {
        (ReplyStatus::Running, 0, _) => t("staff.response_working").to_owned(),
        (ReplyStatus::Running, n, _) => {
            ui::i18n::format("staff.process_running", &[("steps", n.to_string())])
        }
        (_, n, 0) => ui::i18n::format("staff.process_done_short", &[("steps", n.to_string())]),
        (_, n, _) => ui::i18n::format(
            "staff.process_done",
            &[("steps", n.to_string()), ("seconds", seconds)],
        ),
    };
    let header = div()
        .flex()
        .items_center()
        .gap(px(tokens::SPACE_8))
        .child(lead)
        .child(summary)
        .when(!running, |row| {
            row.child(
                Icon::new(if expanded {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                })
                .with_size(px(tokens::LABEL_TEXT)),
            )
        });
    let toggle = {
        let key = key.clone();
        Button::new(("staff-process", index))
            .debug_selector(move || format!("staff-process-{index}"))
            .custom(app.staff_button_style(false, cx))
            .small()
            .h_auto()
            .min_h(px(tokens::BUTTON_HEIGHT))
            .justify_start()
            .px(px(tokens::SPACE_8))
            .text_size(px(tokens::LABEL_TEXT))
            .text_color(rgba(theme.text_secondary))
            .disabled(running)
            .child(header)
            .on_click(cx.listener(move |app, _, _, cx| {
                if !app.staff.expanded.remove(&key) {
                    app.staff.expanded.insert(key.clone());
                }
                app.notify_staff(cx);
            }))
    };
    let mut body = div()
        .id(("staff-process-body", index))
        .flex_none()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(tokens::SPACE_4))
        .child(toggle);
    if expanded {
        let mut list = div()
            .flex()
            .flex_col()
            .gap(px(tokens::SPACE_4))
            .ml(px(tokens::SPACE_12))
            .pl(px(tokens::SPACE_12))
            .border_l_1()
            .border_color(rgba(app.staff_line()))
            .text_size(px(tokens::LABEL_TEXT))
            .text_color(rgba(theme.text_secondary));
        if let Some(plan) = plan {
            list = list.child(plan_block(ctx, plan));
        }
        for part in &reply.parts {
            if let StaffPart::Step {
                id,
                title,
                status,
                elapsed_ms,
            } = part
            {
                let id = id.clone();
                list = list.child(
                    step_row(ctx, title, *status, *elapsed_ms)
                        .debug_selector(move || format!("staff-step-{id}")),
                );
            }
        }
        body = body.child(list);
    }
    Some(body.into_any_element())
}

fn step_row(
    ctx: &BlockCtx,
    title: &norma::protocol::StepTitle,
    status: StepStatus,
    elapsed_ms: Option<u64>,
) -> gpui::Div {
    let theme = ctx.app.staff_theme();
    let (icon, color) = match status {
        StepStatus::Running => (IconName::Loader, theme.text_secondary),
        StepStatus::Completed => (IconName::Check, theme.success),
        StepStatus::Failed => (IconName::TriangleAlert, theme.destructive),
        StepStatus::Denied => (IconName::TriangleAlert, theme.accent_warm),
        StepStatus::Interrupted => (IconName::Minus, theme.text_secondary),
    };
    let note = match status {
        StepStatus::Failed => Some("staff.activity_failed"),
        StepStatus::Denied => Some("staff.activity_denied"),
        StepStatus::Interrupted => Some("staff.activity_interrupted"),
        _ => None,
    };
    div()
        .flex()
        .items_center()
        .gap(px(tokens::SPACE_8))
        .child(
            Icon::new(icon)
                .with_size(px(tokens::LABEL_TEXT))
                .text_color(rgba(color)),
        )
        .child(div().flex_none().child(step_title::label(title)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(tokens::META_TEXT))
                .children(step_title::detail(title).map(str::to_owned)),
        )
        .children(note.map(|key| {
            div()
                .text_size(px(tokens::META_TEXT))
                .text_color(rgba(color))
                .child(t(key))
        }))
        .children(elapsed_ms.map(|elapsed| {
            div()
                .text_size(px(tokens::META_TEXT))
                .child(ui::i18n::format(
                    "staff.activity_duration",
                    &[("seconds", format!("{:.1}", elapsed as f64 / 1000.))],
                ))
        }))
}

fn plan_block(ctx: &BlockCtx, plan: &[PlanItem]) -> gpui::Stateful<gpui::Div> {
    let theme = ctx.app.staff_theme();
    let completed = plan
        .iter()
        .filter(|item| item.status == "completed")
        .count();
    let mut block = div()
        .id("staff-plan")
        .debug_selector(|| "staff-plan".into())
        .flex()
        .flex_col()
        .gap(px(tokens::SPACE_4))
        .child(div().text_size(px(tokens::META_TEXT)).child(format!(
            "{}  {completed}/{}",
            t("staff.execution_plan"),
            plan.len()
        )));
    for item in plan {
        let (icon, color) = match item.status.as_str() {
            "completed" => (IconName::Check, theme.success),
            "in_progress" => (IconName::Loader, theme.accent_cyan),
            _ => (IconName::Minus, theme.text_tertiary),
        };
        block = block.child(
            div()
                .flex()
                .items_center()
                .gap(px(tokens::SPACE_8))
                .child(
                    Icon::new(icon)
                        .with_size(px(tokens::LABEL_TEXT))
                        .text_color(rgba(color)),
                )
                .child(div().flex_1().min_w_0().child(item.text.clone())),
        );
    }
    block
}
