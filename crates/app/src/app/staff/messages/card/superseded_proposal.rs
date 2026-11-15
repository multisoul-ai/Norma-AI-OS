use super::super::*;
use super::shell;
use gpui::rgba;
use norma::protocol::StaffProposal;
use ui::i18n::text as t;

/// 被更新的完整方案替换的提议：保留在原回合里，变灰、没有采纳按钮。
pub(super) fn render(ctx: &BlockCtx, proposal: &StaffProposal) -> AnyElement {
    let theme = ctx.app.staff_theme();
    let id = proposal.id;
    shell::card(ctx, false)
        .debug_selector(move || format!("staff-proposal-card-{id}"))
        .flex_row()
        .items_center()
        .gap(px(tokens::SPACE_8))
        .text_size(px(tokens::LABEL_TEXT))
        .text_color(rgba(theme.text_tertiary))
        .child(div().flex_1().min_w_0().child(proposal.title.clone()))
        .child(
            div()
                .flex_none()
                .text_size(px(tokens::META_TEXT))
                .child(t("staff.proposal_superseded_note")),
        )
        .into_any_element()
}
