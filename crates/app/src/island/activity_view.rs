//! Completion notice: responded face, one sentence, and the countdown bar.

use super::{
    faces::Face,
    style::{self, avatar, Tone},
    view::IslandView,
};
use gpui::{div, prelude::*, px, rgb, Animation, AnimationExt, AnyElement, Context, IntoElement};
use runtime::island::roster::{avatar_face, AgentActivity, ACTIVITY_HOLD_MS};
use std::time::Duration;
use ui::i18n::text;

impl IslandView {
    pub(super) fn completion_face<'a>(faces: &'a [Face], completion_id: &str) -> Option<&'a Face> {
        super::faces::matching_completion_face(faces, completion_id)
    }

    pub(super) fn completion_notice(
        &self,
        face: Option<&Face>,
        label: &str,
        animate: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let name = face.map(|face| face.name.as_str()).unwrap_or(label);
        let task = face.map(|face| face.task.as_str()).unwrap_or("");
        let line = if task.is_empty() {
            ui::i18n::format("island.completed_line_bare", &[("name", name.to_owned())])
        } else {
            ui::i18n::format(
                "island.completed_line",
                &[("name", name.to_owned()), ("task", task.to_owned())],
            )
        };
        let detail = face.and_then(|face| {
            if face.files > 0 {
                Some(ui::i18n::format(
                    "island.file_count",
                    &[("count", face.files.to_string())],
                ))
            } else if !face.detail.is_empty() {
                Some(face.detail.clone())
            } else {
                None
            }
        });
        let (seed, provider) = face
            .map(|face| (face.name.clone(), face.provider.clone()))
            .unwrap_or_else(|| (label.to_owned(), String::new()));
        div()
            .id("island-completed")
            .flex()
            .items_center()
            .gap(px(12.))
            .child(avatar(
                &seed,
                &provider,
                30.,
                avatar_face(AgentActivity::Done),
                animate,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .text_size(px(13.))
                    .child(div().min_w(px(0.)).truncate().child(line))
                    .when_some(detail, |column, detail| {
                        column.child(
                            div()
                                .text_size(px(11.5))
                                .text_color(rgb(style::MUTED))
                                .truncate()
                                .child(detail),
                        )
                    }),
            )
            .child(
                div().flex_none().child(
                    style::action(
                        "island-view-result",
                        text("island.view_result"),
                        Tone::Primary,
                        cx,
                    )
                    .on_click(cx.listener(|view, _, _, cx| {
                        cx.stop_propagation();
                        view.controller.update(cx, |controller, cx| {
                            controller.open_completed(cx);
                        });
                    })),
                ),
            )
    }
}

/// Shrinks on GPUI's frame clock; computing the width per render only moved it at the avatar's 15 fps.
pub(super) fn countdown(deadline_ms: u64, animate: bool) -> AnyElement {
    let bar = div()
        .absolute()
        .bottom(px(0.))
        .left(px(0.))
        .h(px(2.))
        .bg(rgb(style::OK));
    if !animate {
        return bar.w(gpui::relative(1.)).into_any_element();
    }
    bar.with_animation(
        ("island-countdown", deadline_ms),
        Animation::new(Duration::from_millis(ACTIVITY_HOLD_MS)),
        |bar, progress| bar.w(gpui::relative(1. - progress)),
    )
    .into_any_element()
}
