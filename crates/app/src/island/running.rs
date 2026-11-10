//! Expanded running list. One flat row per agent; queued rows have no locate button.

use super::{
    actions::IslandTarget,
    faces::Face,
    style::{self, avatar, Tone},
    view::IslandView,
    IslandProject, IslandWorkspace,
};
use gpui::{div, prelude::*, px, rgb, AnyElement, Context};
use norma::mobile::RunOverviewAttentionKind;
use runtime::island::roster::{accepts, avatar_face, AgentActivity, RunFilter};
use ui::i18n::text;

impl IslandView {
    pub fn face_row(&self, face: &Face, animate: bool, cx: &mut Context<Self>) -> AnyElement {
        let face_style = avatar_face(face.activity);
        let (chip_key, chip_color) = match face.activity {
            AgentActivity::Running => ("island.running", style::RUN),
            AgentActivity::Waiting => ("island.waiting_on_you", style::ATTENTION),
            AgentActivity::Done => ("island.done", style::OK),
            AgentActivity::Queued => ("island.waiting", style::MUTED),
        };
        let route = IslandTarget {
            source: face.source,
            workspace: face.workspace_id.clone(),
            area: face.area_id.clone(),
        };
        let input = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(11.5))
            .line_height(px(14.))
            .text_color(rgb(style::MUTED))
            .child(
                ui::component::Icon::new(ui::component::IconName::User)
                    .size(px(12.))
                    .text_color(rgb(style::FAINT)),
            )
            .child(div().truncate().child(if face.last_input.is_empty() {
                text("island.not_started").to_string()
            } else {
                face.last_input.clone()
            }));
        let running = face.activity == AgentActivity::Running;
        let queued = face.activity == AgentActivity::Queued;
        div()
            .id(gpui::SharedString::from(format!(
                "island-agent-{}",
                face.id
            )))
            .px(px(8.))
            .py(px(7.))
            .rounded(px(12.))
            .hover(|row| row.bg(rgb(0x101010)))
            .flex()
            .items_center()
            .gap(px(10.))
            .child(avatar(&face.name, &face.provider, 30., face_style, animate))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(7.))
                            .text_size(px(13.))
                            .line_height(px(16.))
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(face.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(style::FAINT))
                                    .child(face.provider.clone()),
                            )
                            .child(div().truncate().child(face.session.clone())),
                    )
                    .child(input)
                    .when(face.tool.is_some() || running, |column| {
                        column.child(tool_line(
                            face.tool.as_ref(),
                            running,
                            face.started_at_unix_ms,
                        ))
                    })
                    .when_some(face.progress, |column, (done, total)| {
                        column.child(
                            div()
                                .h(px(3.))
                                .rounded(px(2.))
                                .bg(rgb(0x1d1d1d))
                                .mt(px(3.))
                                .child(
                                    div()
                                        .h_full()
                                        .rounded(px(2.))
                                        .bg(rgb(style::RUN))
                                        .w(gpui::relative(done as f32 / total.max(1) as f32)),
                                ),
                        )
                    }),
            )
            .child(style::chip(text(chip_key), chip_color))
            .when(!queued, |row| {
                row.child(self.locate_button(&route, face.node_id, cx))
            })
            .into_any_element()
    }

    pub fn failure_rows(
        &self,
        workspace: &IslandWorkspace,
        project: &IslandProject,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let route = IslandTarget::new(workspace, project);
        project
            .overview
            .attention
            .iter()
            .filter(|row| {
                matches!(
                    row.kind,
                    RunOverviewAttentionKind::Failed | RunOverviewAttentionKind::Takeover
                )
            })
            .map(|row| {
                style::card()
                    .flex_row()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(0xff9678))
                                    .truncate()
                                    .child(format!("{} · {}", row.agent, row.project)),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(style::MUTED))
                                    .truncate()
                                    .child(row.title.clone()),
                            ),
                    )
                    .child(self.locate_button(&route, row.node_id, cx))
                    .into_any_element()
            })
            .collect()
    }

    pub fn locate_button(
        &self,
        route: &IslandTarget,
        node: u64,
        cx: &mut Context<Self>,
    ) -> ui::component::button::Button {
        let route = route.clone();
        style::action(
            format!(
                "island-locate-{}-{}-{}-{node}",
                route.source, route.workspace, route.area
            ),
            text("island.locate"),
            Tone::Ghost,
            cx,
        )
        .h(px(22.))
        .px(px(8.))
        .on_click(cx.listener(move |view, _, _, cx| {
            view.controller.update(cx, |controller, cx| {
                controller.locate(route.clone(), node, cx)
            });
        }))
    }
}

pub(super) fn matches_filter(filter: RunFilter, face: &Face) -> bool {
    accepts(filter, face.activity)
}

/// Third row: the latest tool (or just "running" before the first one). Mono, one line;
/// the elapsed time only while running.
fn tool_line(
    tool: Option<&norma::mobile::RecentTool>,
    running: bool,
    started_at_unix_ms: Option<u64>,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .font_family("Menlo")
        .text_size(px(11.))
        .line_height(px(14.))
        .text_color(rgb(style::FAINT))
        .child(if running {
            div()
                .flex_none()
                .size(px(10.))
                .rounded(px(5.))
                .border_1()
                .border_color(rgb(style::RUN))
        } else {
            div()
                .flex_none()
                .mx(px(2.))
                .size(px(6.))
                .rounded(px(3.))
                .bg(rgb(style::FAINT))
        })
        .child(
            div()
                .flex_none()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(style::MUTED))
                .child(tool.map_or_else(|| text("island.running").to_string(), |t| t.name.clone())),
        )
        .when_some(tool.and_then(|t| t.arg.clone()), |line, arg| {
            line.child(div().truncate().child(arg))
        })
        .when_some(started_at_unix_ms.filter(|_| running), |line, start| {
            line.child(div().flex_none().ml_auto().child(ui::i18n::format(
                "island.elapsed_minutes",
                &[("count", elapsed_minutes(start).to_string())],
            )))
        })
        .into_any_element()
}

fn elapsed_minutes(start: u64) -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    now.saturating_sub(start) / 60_000
}
