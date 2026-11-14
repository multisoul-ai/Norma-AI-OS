use super::super::*;
use gpui::rgba;
use ui::i18n::text as t;

pub(in crate::app::staff::messages) struct Notice<'a>(pub &'a TaskMessage);

impl MessageBlock for Notice<'_> {
    fn render(
        &self,
        ctx: &BlockCtx,
        _window: &mut Window,
        _cx: &mut Context<MultisoulApp>,
    ) -> AnyElement {
        let theme = ctx.app.staff_theme();
        let message = self.0;
        let mut row = div().w_full().flex().flex_col().gap(px(tokens::SPACE_8));
        if let TaskMessage::WorkerStarted { .. } | TaskMessage::Reopened = message {
            row = row
                .px(px(tokens::SPACE_16))
                .py(px(tokens::SPACE_12))
                .rounded(px(tokens::SPACE_16))
                .bg(rgba(ui::icons::with_alpha(theme.panel_high, 140)))
                .text_size(px(tokens::SPACE_12))
                .text_color(rgba(theme.text_secondary))
                .child(if matches!(message, TaskMessage::Reopened) {
                    t("staff.reopened")
                } else {
                    t("staff.worker_started")
                });
        }
        row.into_any_element()
    }
}
