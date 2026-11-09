//! Avatar queue under an alert card. Clicking one shows that card.

use super::{
    faces::QueueItem,
    style::{self, avatar},
    view::IslandView,
};
use gpui::{div, prelude::*, px, rgb, rgba, Context, IntoElement};
use runtime::island::bloub::Mood;
use runtime::island::roster::{avatar_face, AgentActivity, AvatarFace};
use ui::{
    component::button::{Button, ButtonCustomVariant, ButtonVariants},
    i18n::text,
};

impl IslandView {
    pub(super) fn pending_queue(
        &self,
        items: &[QueueItem],
        selected: Option<&str>,
        animate: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let mut row = div()
            .id("island-pending-queue")
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(4.))
            .text_size(px(11.5))
            .text_color(rgb(style::MUTED))
            .child(text("island.pending_queue"));
        for (index, item) in items.iter().enumerate() {
            let id = item.id.clone();
            let review = item.review;
            let proposal = item.proposal;
            let on = selected == Some(item.id.as_str());
            // Selected card waits. The rest stay idle at full opacity; queued 0.55 is only for the running list.
            let mood = if on {
                avatar_face(AgentActivity::Waiting)
            } else {
                AvatarFace {
                    mood: Mood::Idle,
                    opacity: 1.,
                }
            };
            row = row.child(
                Button::new(gpui::SharedString::from(format!("island-queue-{index}")))
                    .p(px(3.))
                    .rounded(px(12.))
                    .child(avatar(&item.name, &item.provider, 22., mood, animate))
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .color(if on {
                                rgb(0x1d1d1d).into()
                            } else {
                                rgba(0).into()
                            })
                            .foreground(rgb(style::FG).into())
                            .border(if on {
                                rgb(0x3a3a3a).into()
                            } else {
                                rgba(0).into()
                            })
                            .hover(rgb(0x161616).into())
                            .active(rgb(0x303030).into()),
                    )
                    .on_click(cx.listener(move |view, _, _, cx| {
                        cx.stop_propagation();
                        let id = id.clone();
                        let display = view.display;
                        view.controller.update(cx, |state, cx| {
                            if proposal {
                                state.focused_review = None;
                                state.event(display, runtime::island::IslandEvent::Expand, cx);
                                state.event(
                                    display,
                                    runtime::island::IslandEvent::SelectTab(
                                        runtime::island::IslandTab::Pending,
                                    ),
                                    cx,
                                );
                            } else if review {
                                state.focused_review = Some(id.clone());
                                state.event(
                                    display,
                                    runtime::island::IslandEvent::ReviewHold(true),
                                    cx,
                                );
                            } else {
                                state.focused_review = None;
                                state.event(
                                    display,
                                    runtime::island::IslandEvent::SelectAlert(id),
                                    cx,
                                );
                            }
                        });
                    })),
            );
        }
        let others = items.len().saturating_sub(1);
        row.child(div().ml_auto().child(ui::i18n::format(
            "island.pending_more",
            &[("count", others.to_string())],
        )))
    }
}
