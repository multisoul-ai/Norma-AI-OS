//! Completion notice: responded face, one sentence, and the countdown bar.

use super::{
    faces::Face,
    style::{self, avatar, Tone},
    view::IslandView,
};
use gpui::{div, prelude::*, px, rgb, AnyElement, Context, IntoElement};
use runtime::island::roster::{avatar_face, countdown_scale, AgentActivity};
use ui::i18n::text;

impl IslandView {
    pub(super) fn completion_face<'a>(faces: &'a [Face], completion_id: &str) -> Option<&'a Face> {
        faces
            .iter()
            .find(|face| format!("{}:{}", face.source, face.node_id) == completion_id)
            .or_else(|| {
                faces
                    .iter()
                    .find(|face| face.activity == AgentActivity::Done)
            })
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
                    .child(line)
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
                style::action(
                    "island-view-result",
                    text("island.view_result"),
                    Tone::Primary,
                    cx,
                )
                .on_click(cx.listener(|view, _, _, cx| {
                    cx.stop_propagation();
                    view.event(runtime::island::IslandEvent::Expand, cx);
                    view.event(
                        runtime::island::IslandEvent::SelectTab(
                            runtime::island::IslandTab::Pending,
                        ),
                        cx,
                    );
                })),
            )
    }
}

pub(super) fn countdown(now_ms: u64, deadline_ms: u64, animate: bool) -> AnyElement {
    let scale = countdown_scale(now_ms, deadline_ms, !animate);
    div()
        .id("island-countdown")
        .absolute()
        .bottom(px(0.))
        .left(px(0.))
        .h(px(2.))
        .w(gpui::relative(scale as f32))
        .bg(rgb(style::OK))
        .into_any_element()
}
