//! 回合操作：复制本回合正文；重新生成只出现在最后一条用户触发的回复上。
use crate::app::staff::messages::{tokens, BlockCtx};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, AnyElement, ClipboardItem, Context};
use norma::protocol::{ReplyTrigger, StaffReply, TaskMessage};
use ui::component::{
    button::{Button, ButtonVariants},
    Disableable, Sizable,
};
use ui::i18n::text as t;

pub(super) fn render(
    ctx: &BlockCtx,
    reply: &StaffReply,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    let (app, index) = (ctx.app, ctx.index);
    let theme = app.staff_theme();
    let last_reply = ctx
        .task
        .messages
        .iter()
        .rposition(|message| matches!(message, TaskMessage::Staff(_)));
    let can_regenerate = last_reply == Some(index) && reply.trigger == ReplyTrigger::User;
    let text = reply.text();
    let task_id = ctx.task.id.clone();
    let button = |id: &'static str| {
        Button::new((id, index))
            .custom(app.staff_button_style(false, cx))
            .small()
            .h(px(28.))
            .px(px(tokens::SPACE_8))
            .text_size(px(tokens::META_TEXT))
            .text_color(rgba(theme.text_secondary))
    };
    div()
        .debug_selector(move || format!("staff-reply-actions-{index}"))
        .flex()
        .items_center()
        .gap(px(tokens::SPACE_4))
        .when(!text.is_empty(), |row| {
            row.child(
                button("staff-copy")
                    .debug_selector(|| "staff-copy".into())
                    .label(t("staff.copy"))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                        app.show_staff_toast(ui::i18n::message("staff.copied", &[]), None, cx);
                    })),
            )
        })
        .when(can_regenerate, |row| {
            row.child(
                button("staff-regenerate")
                    .debug_selector(|| "staff-regenerate".into())
                    .label(t("staff.regenerate"))
                    .disabled(app.staff.busy)
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.regenerate_staff_reply(&task_id, cx);
                    })),
            )
        })
        .into_any_element()
}
