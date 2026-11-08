//! Compact capsule: running faces on the left, pending faces on the right.

use super::{
    faces::Face,
    style::{self, avatar, avatar_pinging},
    view::IslandView,
};
use gpui::{div, prelude::*, px, rgb, rgba, Context, Div};
use runtime::island::roster::{avatar_face, compact_roster, AgentActivity};
use ui::i18n::text;

const AVATAR: f32 = 20.;

impl IslandView {
    pub(super) fn compact_slots(
        &self,
        faces: &[Face],
        animate: bool,
        cx: &Context<Self>,
    ) -> (Div, Div) {
        let _ = cx;
        let roster = compact_roster(&super::faces::slots(faces));
        let by_id = |id: &str| faces.iter().find(|face| face.id == id);
        let mut left = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .flex_1()
            .min_w(px(0.));
        if roster.running_total() == 0 {
            left = left.child(
                div()
                    .text_color(rgb(style::FAINT))
                    .child(text("island.idle")),
            );
        } else {
            left = left
                .child(
                    div()
                        .size(px(10.))
                        .rounded(px(5.))
                        .border_1()
                        .border_color(rgb(style::RUN)),
                )
                .child(
                    div()
                        .id("island-running-count")
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(roster.running_total().to_string()),
                );
            let mut stack = div().flex().items_center();
            for (index, id) in roster.running.iter().enumerate() {
                let Some(face) = by_id(id) else { continue };
                stack = stack.child(
                    avatar(
                        &face.name,
                        &face.provider,
                        AVATAR,
                        avatar_face(face.activity),
                        animate,
                    )
                    .when(index > 0, |slot| slot.ml(px(-2.))),
                );
            }
            if roster.running_overflow > 0 {
                stack = stack.child(
                    div()
                        .id("island-overflow")
                        .ml(px(-4.))
                        .h(px(16.))
                        .min_w(px(16.))
                        .px(px(4.))
                        .rounded(px(8.))
                        .bg(rgb(0x23272f))
                        .text_size(px(9.5))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(format!("+{}", roster.running_overflow)),
                );
            }
            left = left.child(stack);
        }
        let mut right = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(6.))
            .flex_1()
            .min_w(px(0.));
        if roster.pending_total == 0 {
            right = right.child(
                ui::component::Icon::new(ui::component::IconName::Check)
                    .size(px(13.))
                    .text_color(rgb(style::OK)),
            );
        } else {
            let mut stack = div().flex().items_center();
            for (index, id) in roster.pending.iter().enumerate() {
                let Some(face) = by_id(id) else { continue };
                // The prototype draws every right-slot face as waiting, with an orange ring.
                let waiting = avatar_face(AgentActivity::Waiting);
                stack = stack.child(
                    div()
                        .rounded(px(11.))
                        .border_1()
                        .border_color(rgb(style::ATTENTION))
                        .when(index > 0, |slot| slot.ml(px(-2.)))
                        .child(avatar_pinging(
                            &face.name,
                            &face.provider,
                            AVATAR,
                            waiting,
                            animate,
                        )),
                );
            }
            right = right.child(stack).child(
                div()
                    .id("island-pending-count")
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .h(px(16.))
                    .min_w(px(16.))
                    .px(px(5.))
                    .rounded(px(8.))
                    .bg(rgba(0xffa94d29))
                    .text_color(rgb(style::ATTENTION))
                    .text_size(px(11.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(div().size(px(6.)).rounded(px(3.)).bg(rgb(style::ATTENTION)))
                    .child(roster.pending_total.to_string()),
            );
        }
        (left, right)
    }
}
