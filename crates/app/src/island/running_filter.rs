//! Running-tab filter. Queued agents stay in All and out of the other chips.

use super::{
    faces::Face,
    style::{self, avatar},
    view::IslandView,
};
use gpui::{div, prelude::*, px, rgb, rgba, Context, Div};
use runtime::island::roster::{avatar_face, filter_counts, RunFilter};
use ui::{
    component::button::{Button, ButtonCustomVariant, ButtonVariants},
    i18n::text,
};

const FILTERS: [(RunFilter, &str, u32); 4] = [
    (RunFilter::All, "island.filter_all", 0xf4f6f8),
    (RunFilter::Running, "island.running", style::RUN),
    (
        RunFilter::Waiting,
        "island.filter_waiting",
        style::ATTENTION,
    ),
    (RunFilter::Done, "island.filter_done", style::OK),
];

impl IslandView {
    pub(super) fn run_filter_bar(&self, faces: &[Face], animate: bool, cx: &Context<Self>) -> Div {
        let selected = self.controller.read(cx).run_filter;
        let counts = filter_counts(&faces.iter().map(|face| face.activity).collect::<Vec<_>>());
        let numbers = [counts.all, counts.running, counts.waiting, counts.done];
        let mut bar = div().flex().items_center().gap(px(6.)).flex_wrap();
        for (index, ((filter, key, color), count)) in FILTERS.into_iter().zip(numbers).enumerate() {
            let on = selected == filter;
            bar = bar.child(
                Button::new(gpui::SharedString::from(format!("island-filter-{index}")))
                    .h(px(26.))
                    .px(px(10.))
                    .rounded(px(13.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(12.))
                            .child(div().size(px(7.)).rounded(px(4.)).bg(rgb(color)))
                            .child(format!("{} {count}", text(key))),
                    )
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .color(if on {
                                rgb(0x242424).into()
                            } else {
                                rgba(0).into()
                            })
                            .foreground(rgb(if on { style::FG } else { style::MUTED }).into())
                            .border(if on {
                                rgba(0).into()
                            } else {
                                rgb(0x2a2a2e).into()
                            })
                            .hover(rgb(0x1d1d1d).into())
                            .active(rgb(0x303030).into()),
                    )
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.controller.update(cx, |state, cx| {
                            state.run_filter = filter;
                            cx.notify();
                        });
                    })),
            );
        }
        let mut overview = div()
            .flex()
            .items_center()
            .flex_none()
            .w(px(runtime::island::roster::overview_strip_width(
                faces.len(),
            )))
            .h(px(20.))
            .ml_auto();
        for (index, face) in faces.iter().enumerate() {
            overview = overview.child(
                avatar(
                    &face.name,
                    &face.provider,
                    20.,
                    avatar_face(face.activity),
                    animate,
                )
                .when(index > 0, |slot| slot.ml(px(-2.))),
            );
        }
        bar.child(overview)
    }
}
