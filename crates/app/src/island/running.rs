use super::{
    actions::IslandTarget,
    style::{self, Tone},
    view::IslandView,
    IslandProject, IslandWorkspace,
};
use gpui::{div, prelude::*, px, rgb, AnyElement, Context};
use norma::mobile::{RunOverviewAgent, RunOverviewAttentionKind};
use runtime::island::bloub::Mood;
use ui::{
    component::{
        button::{Button, ButtonCustomVariant, ButtonVariants},
        Sizable,
    },
    i18n::text,
};

impl IslandView {
    pub fn render_running(
        &mut self,
        workspace: &IslandWorkspace,
        project: &IslandProject,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let route = IslandTarget::new(workspace, project);
        let mut rows: Vec<_> = project
            .overview
            .running
            .iter()
            .map(|row| self.running_row(&route, row, cx))
            .collect();
        rows.extend(
            project
                .overview
                .idle
                .iter()
                .filter(|row| row.is_completed())
                .map(|row| self.running_row(&route, row, cx)),
        );
        for row in &project.overview.attention {
            if !matches!(
                row.kind,
                RunOverviewAttentionKind::Failed | RunOverviewAttentionKind::Takeover
            ) {
                continue;
            }
            rows.push(
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
                    .into_any_element(),
            );
        }
        let waiting: Vec<_> = project
            .overview
            .idle
            .iter()
            .filter(|row| !row.is_completed())
            .collect();
        if !waiting.is_empty() {
            let key = format!("{}:{}:{}", route.source, route.workspace, route.area);
            let open = self.controller.read(cx).idle_open.contains(&key);
            rows.push(
                Button::new(gpui::SharedString::from(format!("idle-{key}")))
                    .small()
                    .w_full()
                    .h(px(32.))
                    .px(px(8.))
                    .rounded(px(10.))
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .color(gpui::rgba(0).into())
                            .foreground(rgb(style::MUTED).into())
                            .border(gpui::rgba(0).into())
                            .hover(rgb(0x161616).into())
                            .active(rgb(0x242424).into()),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(div().flex().children(waiting.iter().take(4).map(|row| {
                                style::avatar_mood(&row.name, &row.profile, 16., Mood::Waiting)
                            })))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .text_size(px(12.5))
                                    .truncate()
                                    .child(ui::i18n::format(
                                        "island.waiting_names",
                                        &[(
                                            "names",
                                            waiting
                                                .iter()
                                                .map(|row| row.name.as_str())
                                                .collect::<Vec<_>>()
                                                .join(", "),
                                        )],
                                    )),
                            )
                            .child(
                                ui::component::Icon::new(if open {
                                    ui::component::IconName::ChevronDown
                                } else {
                                    ui::component::IconName::ChevronRight
                                })
                                .size(px(12.)),
                            ),
                    )
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.controller.update(cx, |state, cx| {
                            if !state.idle_open.remove(&key) {
                                state.idle_open.insert(key.clone());
                            }
                            cx.notify();
                        });
                    }))
                    .into_any_element(),
            );
            if open {
                rows.extend(waiting.iter().map(|row| self.running_row(&route, row, cx)));
            }
        }
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
        rows
    }

    fn running_row(
        &self,
        route: &IslandTarget,
        row: &RunOverviewAgent,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let done = row.is_completed();
        let running = row.presence == norma::mobile::RunOverviewPresence::Running;
        let color = if done {
            style::OK
        } else if running {
            style::RUN
        } else {
            style::MUTED
        };
        let elapsed = row.started_at_unix_ms.filter(|_| running).map(|start| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let seconds = now.saturating_sub(start) / 1000;
            format!("{}:{:02}", seconds / 60, seconds % 60)
        });
        let mut detail = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(11.5))
            .text_color(rgb(style::MUTED))
            .child(if done {
                div().flex_none().child(
                    ui::component::Icon::new(ui::component::IconName::Check)
                        .size(px(12.))
                        .text_color(rgb(color)),
                )
            } else {
                div()
                    .size(px(10.))
                    .rounded(px(5.))
                    .border_1()
                    .border_color(rgb(color))
                    .flex_none()
            })
            .child(
                div()
                    .truncate()
                    .child(row.action.clone().unwrap_or_else(|| {
                        text(if done {
                            "island.done"
                        } else if running {
                            "island.running"
                        } else {
                            "island.waiting"
                        })
                        .into()
                    })),
            );
        if let Some(elapsed) = elapsed {
            detail = detail.child(div().flex_none().child(elapsed));
        }
        let main = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap(px(3.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .text_size(px(13.))
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(row.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(style::FAINT))
                            .child(row.profile.clone()),
                    )
                    .when_some(
                        row.latest_prompt.clone().or(row.task.clone()),
                        |v, title| v.child(div().truncate().child(title)),
                    ),
            )
            .child(detail)
            .when_some(row.progress.as_ref(), |view, progress| {
                view.child(
                    div()
                        .h(px(3.))
                        .rounded(px(2.))
                        .bg(rgb(0x1d1d1d))
                        .mt(px(3.))
                        .child(div().h_full().rounded(px(2.)).bg(rgb(style::RUN)).w(
                            gpui::relative(
                                progress.completed as f32 / progress.total.max(1) as f32,
                            ),
                        )),
                )
            });
        div()
            .id(gpui::SharedString::from(format!(
                "island-agent-{}-{}-{}",
                route.source, route.area, row.node_id
            )))
            .px(px(8.))
            .py(px(9.))
            .rounded(px(12.))
            .hover(|s| s.bg(rgb(0x101010)))
            .flex()
            .items_start()
            .gap(px(10.))
            .child(style::avatar_mood(
                &row.name,
                &row.profile,
                22.,
                if done {
                    Mood::Responded
                } else if running {
                    Mood::Working
                } else {
                    Mood::Waiting
                },
            ))
            .child(main)
            .child(style::chip(
                text(if done {
                    "island.done"
                } else if running {
                    "island.running"
                } else {
                    "island.waiting"
                }),
                color,
            ))
            .child(self.locate_button(route, row.node_id, cx))
            .into_any_element()
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

/// Counts describe only the loaded agent rows rendered below this workspace heading.
pub(super) fn workspace_summary(workspace: &IslandWorkspace) -> String {
    let running: usize = workspace
        .projects
        .iter()
        .map(|p| p.overview.running.len())
        .sum();
    let done = workspace
        .projects
        .iter()
        .flat_map(|p| &p.overview.idle)
        .filter(|r| r.is_completed())
        .count();
    let waiting = workspace
        .projects
        .iter()
        .flat_map(|p| &p.overview.idle)
        .filter(|r| !r.is_completed())
        .count();
    [
        ("island.running_count", running),
        ("island.done_count", done),
        ("island.waiting_count", waiting),
    ]
    .into_iter()
    .filter(|(_, count)| *count > 0)
    .map(|(key, count)| ui::i18n::format(key, &[("count", count.to_string())]))
    .collect::<Vec<_>>()
    .join(" · ")
}
