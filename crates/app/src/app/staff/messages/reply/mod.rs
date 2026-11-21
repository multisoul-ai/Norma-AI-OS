//! 幕僚回复：一个署名 → 过程区 → 正文 → 内嵌卡片 → 操作。流式时就地更新同一条。
use super::*;
use gpui::rgba;
use norma::protocol::{ReplyStatus, StaffPart, StaffReply};
use ui::component::{spinner::Spinner, Icon, IconName, Sizable};
use ui::i18n::text as t;

mod actions;
mod process;
mod step_title;

pub(in crate::app::staff::messages) struct Reply<'a>(pub &'a StaffReply);

impl MessageBlock for Reply<'_> {
    fn render(
        &self,
        ctx: &BlockCtx,
        window: &mut Window,
        cx: &mut Context<MultisoulApp>,
    ) -> AnyElement {
        let reply = self.0;
        let (task, index) = (ctx.task, ctx.index);
        let theme = ctx.app.staff_theme();
        let running = reply.is_running();
        let mut body = div()
            .group(gpui::SharedString::from(format!("staff-reply-{index}")))
            .w_full()
            .flex()
            .flex_col()
            .gap(px(tokens::SPACE_12));
        // 这条回复是横向行里的一项。交叉轴用内容高度，避免百分比高度去对行高。
        body.style().align_self = Some(gpui::AlignSelf::FlexStart);
        body = body
            .child(super::text::staff_byline(ctx.app))
            .children(process::render(ctx, reply, cx));
        if running && reply.parts.is_empty() {
            let size = px(tokens::BODY_TEXT);
            body = body.child(
                div()
                    .id("staff-response-status")
                    .debug_selector(|| "staff-response-status".into())
                    .flex()
                    .items_center()
                    .gap(px(tokens::SPACE_8))
                    .text_size(px(tokens::LABEL_TEXT))
                    .text_color(rgba(theme.text_secondary))
                    .child(if ctx.app.motion.reduce_motion {
                        Icon::new(IconName::Loader)
                            .with_size(size)
                            .into_any_element()
                    } else {
                        Spinner::new()
                            .with_size(size)
                            .color(rgba(theme.accent_cyan).into())
                            .into_any_element()
                    })
                    .child(t("staff.response_waiting")),
            );
        }
        let last_text = reply
            .parts
            .iter()
            .rposition(|part| matches!(part, StaffPart::Text(_)));
        crate::app::staff::register_link_target(window, cx);
        for (position, part) in reply.parts.iter().enumerate() {
            body = match part {
                StaffPart::Text(text) => {
                    let streaming = running && last_text == Some(position);
                    let markdown = if streaming {
                        text.clone()
                    } else {
                        crate::preview::rich_link::linkify_inline_paths(text)
                    };
                    body.child(
                        div()
                            .debug_selector(move || format!("staff-reply-text-{index}-{position}"))
                            .w_full()
                            .block()
                            .flex_none()
                            .child(
                                ui::component::text::TextView::markdown(
                                    gpui::SharedString::from(format!(
                                        "staff-markdown-{}-{index}-{position}",
                                        task.id
                                    )),
                                    markdown,
                                    window,
                                    cx,
                                )
                                .selectable(true)
                                .h_auto()
                                .flex_none(),
                            ),
                    )
                }
                StaffPart::Proposal(id) => {
                    body.child(card::proposal::Proposal(*id).render(ctx, window, cx))
                }
                StaffPart::Question(id) => match task.question(id) {
                    Some(question) => {
                        body.child(card::question::Question(question).render(ctx, window, cx))
                    }
                    None => body,
                },
                StaffPart::Step { .. } | StaffPart::Plan(_) => body,
            };
        }
        let note = match reply.status {
            ReplyStatus::Stopped => Some("staff.reply_stopped"),
            ReplyStatus::Failed => Some("staff.response_failed"),
            ReplyStatus::Running | ReplyStatus::Done => None,
        };
        if let Some(key) = note {
            body = body.child(
                div()
                    .debug_selector(|| "staff-reply-note".into())
                    .text_size(px(tokens::LABEL_TEXT))
                    .text_color(rgba(theme.accent_warm))
                    .child(t(key)),
            );
        }
        if !running {
            body = body.child(actions::render(ctx, reply, cx));
        }
        body.into_any_element()
    }
}
