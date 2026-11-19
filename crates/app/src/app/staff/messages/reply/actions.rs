//! 回合操作：复制本回合正文；重新生成只出现在最后一条用户触发的回复上。
use crate::app::staff::messages::{tokens, BlockCtx};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, AnyElement, ClipboardItem, Context};
use norma::protocol::{ReplyTrigger, StaffReply, TaskMessage};
use ui::component::{
    button::{Button, ButtonVariants},
    Disableable, Icon, IconName, Sizable,
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
    let button = |id: &'static str, icon: IconName| {
        Button::new((id, index))
            .custom(app.staff_button_style(false, cx))
            .small()
            .w(px(28.))
            .h(px(28.))
            .rounded(px(tokens::SPACE_8))
            .icon(
                Icon::new(icon)
                    .size(px(15.))
                    .text_color(rgba(theme.text_secondary)),
            )
    };
    let group = gpui::SharedString::from(format!("staff-reply-{index}"));
    div()
        .debug_selector(move || format!("staff-reply-actions-{index}"))
        .flex()
        .items_center()
        .gap(px(tokens::SPACE_4))
        // 悬停回复时才显示；占位不变，不会让内容跳动。
        .opacity(0.)
        .group_hover(group, |style| style.opacity(1.))
        .when(!text.is_empty(), |row| {
            row.child(
                button("staff-copy", IconName::Copy)
                    .debug_selector(|| "staff-copy".into())
                    .tooltip(t("staff.copy"))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                        app.show_staff_toast(ui::i18n::message("staff.copied", &[]), None, cx);
                    })),
            )
        })
        .when(can_regenerate, |row| {
            row.child(
                button("staff-regenerate", IconName::Redo2)
                    .debug_selector(|| "staff-regenerate".into())
                    .tooltip(t("staff.regenerate"))
                    .disabled(app.staff.busy)
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.regenerate_staff_reply(&task_id, cx);
                    })),
            )
        })
        .into_any_element()
}
