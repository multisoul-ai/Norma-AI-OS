use super::super::*;
use gpui::rgba;
use ui::component::{text::TextView, StyledExt};
use ui::i18n::text as t;

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
        let theme = ctx.app.staff_theme();
        let message = self.0;
        let mut row = div().w_full().flex().flex_col().gap(px(tokens::SPACE_8));
        if let TaskMessage::Norma { text } = message {
            row = row
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(tokens::SPACE_8))
                        .text_size(px(tokens::LABEL_TEXT))
                        .font_semibold()
                        .child(crate::app::chrome::brand_logo())
                        .child("Norma")
                        .child(
                            div()
                                .text_size(px(tokens::META_TEXT))
                                .font_normal()
                                .text_color(rgba(theme.text_secondary))
                                .child(t("staff.coordination")),
                        ),
                )
                .child(
                    TextView::markdown(
                        gpui::SharedString::from(format!("staff-markdown-{}-{index}", task.id)),
                        text.clone(),
                        window,
                        cx,
                    )
                    .selectable(true),
                );
        }
        row.into_any_element()
    }
}
