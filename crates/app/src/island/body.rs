//! Island body under the header. The view keeps layout and the morph.

use super::{
    faces::{Face, QueueItem},
    style,
    view::IslandView,
    IslandWorkspace,
};
use gpui::{div, prelude::*, px, rgb, Context, Div, Window};
use runtime::island::roster::{pending_queue_visible, RunFilter};
use runtime::island::{IslandMode, IslandTab};
use ui::i18n::text;

pub(super) struct BodyInput<'a> {
    pub mode: IslandMode,
    pub tab: IslandTab,
    pub workspaces: &'a [IslandWorkspace],
    pub faces: &'a [Face],
    pub queue: &'a [QueueItem],
    pub pending: usize,
    pub message: Option<ui::i18n::LocalizedText>,
    pub confirmation: bool,
    pub completion_label: &'a str,
    pub animate: bool,
}

impl IslandView {
    pub(super) fn island_body(
        &mut self,
        input: BodyInput<'_>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let BodyInput {
            mode,
            tab,
            workspaces,
            faces,
            queue,
            pending,
            message,
            confirmation,
            completion_label,
            animate,
        } = input;
        let mut body = div()
            .w(px(self.screen.layout(mode).frame.width as f32))
            .flex_none()
            .px(px(16.))
            .pt(px(match mode {
                IslandMode::Activity => 4.,
                IslandMode::Alert => 6.,
                _ => 12.,
            }))
            .pb(px(if mode == IslandMode::Activity {
                14.
            } else {
                0.
            }))
            .flex()
            .flex_col()
            .gap(px(if mode == IslandMode::Expanded {
                14.
            } else {
                10.
            }));
        if let Some(message) = message {
            body = body.child(
                div()
                    .id("island-feedback")
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .when(confirmation, |row| {
                        row.child(
                            ui::component::Icon::new(ui::component::IconName::Check)
                                .size(px(14.))
                                .text_color(rgb(style::OK)),
                        )
                    })
                    .child(message.to_string())
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.event(runtime::island::IslandEvent::Expand, cx)
                    })),
            );
        }
        if mode == IslandMode::Activity {
            if !confirmation {
                let id = self
                    .controller
                    .read(cx)
                    .displays
                    .state
                    .activity_id()
                    .unwrap_or("")
                    .to_owned();
                let face = Self::completion_face(faces, &id);
                body = body.child(self.completion_notice(face, completion_label, animate, cx));
            }
            return body;
        }
        if tab == IslandTab::Staff && mode == IslandMode::Expanded {
            return body.child(self.staff_tab(window, cx));
        }
        if mode == IslandMode::Alert {
            return self.alert_body(body, workspaces, queue, animate, cx);
        }
        if mode == IslandMode::Expanded && tab == IslandTab::Running {
            return self.running_body(body, workspaces, faces, animate, cx);
        }
        if mode == IslandMode::Expanded && tab == IslandTab::Pending && pending == 0 {
            body = body.child(style::empty("island.no_pending", "island.no_pending_hint"));
        }
        for workspace in workspaces {
            let mut rows = Vec::new();
            for project in &workspace.projects {
                rows.extend(self.render_pending(workspace, project, false, cx));
            }
            if rows.is_empty() {
                continue;
            }
            body = body.child(div().flex().flex_col().gap(px(14.)).children(rows));
        }
        body
    }

    fn alert_body(
        &mut self,
        mut body: Div,
        workspaces: &[IslandWorkspace],
        queue: &[QueueItem],
        animate: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let focus = self.controller.read(cx).focused_review.clone();
        let alert_id = self
            .controller
            .read(cx)
            .displays
            .state
            .alert_id()
            .map(str::to_owned);
        for workspace in workspaces {
            for project in &workspace.projects {
                if let Some(review) = focus.as_deref() {
                    body =
                        body.children(self.render_artifacts(workspace, project, Some(review), cx));
                } else {
                    body = body.children(self.render_pending(workspace, project, true, cx));
                }
            }
        }
        if pending_queue_visible(queue.len()) {
            let selected = focus.as_deref().or(alert_id.as_deref());
            body = body.child(self.pending_queue(queue, selected, animate, cx));
        }
        body
    }

    fn running_body(
        &mut self,
        mut body: Div,
        workspaces: &[IslandWorkspace],
        faces: &[Face],
        animate: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let filter = self.controller.read(cx).run_filter;
        body = body.child(self.run_filter_bar(faces, animate, cx));
        let mut groups = div().flex().flex_col().gap(px(8.));
        let mut any = false;
        for workspace in workspaces {
            let shown: Vec<_> = faces
                .iter()
                .filter(|face| {
                    face.workspace_id == workspace.id
                        && super::running::matches_filter(filter, face)
                })
                .collect();
            let mut rows = Vec::new();
            for face in &shown {
                rows.push(self.face_row(face, animate, cx));
            }
            if filter == RunFilter::All {
                for project in &workspace.projects {
                    rows.extend(self.failure_rows(workspace, project, cx));
                    for unloaded in &project.overview.unloaded_projects {
                        rows.push(
                            div()
                                .px(px(8.))
                                .py(px(9.))
                                .text_size(px(12.))
                                .text_color(rgb(style::MUTED))
                                .child(format!("{} · {}", unloaded.name, text("island.not_loaded")))
                                .into_any_element(),
                        );
                    }
                }
            }
            if rows.is_empty() {
                continue;
            }
            any = true;
            groups = groups.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .px(px(4.))
                            .text_size(px(11.5))
                            .line_height(px(14.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(style::MUTED))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(workspace.name.clone())
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::NORMAL)
                                    .text_color(rgb(style::FAINT))
                                    .child(ui::i18n::format(
                                        "island.agent_count",
                                        &[("count", shown.len().to_string())],
                                    )),
                            ),
                    )
                    .children(rows),
            );
        }
        if any {
            body = body.child(groups);
        } else {
            body = body.child(style::empty("island.no_running", "island.no_running_hint"));
        }
        body
    }
}
