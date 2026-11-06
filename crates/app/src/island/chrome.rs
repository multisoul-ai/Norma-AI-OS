use super::{
    style::{self, Tone},
    view::IslandView,
    IslandWorkspace,
};
use gpui::{div, prelude::*, px, rgb, rgba, svg, Context, Div, FontWeight};
use runtime::island::{geometry::IslandLayout, IslandEvent, IslandMode, IslandTab};
use ui::{
    component::button::{Button, ButtonCustomVariant, ButtonVariants},
    i18n::text,
};

impl IslandView {
    pub fn header(
        &self,
        layout: IslandLayout,
        workspaces: &[IslandWorkspace],
        running: usize,
        pending: usize,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let controller = self.controller.read(cx);
        let mode = controller.displays.mode_on(self.display);
        let pinned = controller.displays.state.pinned();
        let mut left = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .flex_1()
            .min_w(px(0.));
        let mut right = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(6.))
            .flex_1()
            .min_w(px(0.));
        if mode == IslandMode::Expanded {
            left = left
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(text("island.overview")),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(style::FAINT))
                        .child(ui::i18n::format(
                            "island.workspace_count",
                            &[("count", workspaces.len().to_string())],
                        )),
                );
            right = right.child(
                Button::new("island-pin")
                    .w(px(26.))
                    .h(px(24.))
                    .p(px(0.))
                    .rounded(px(7.))
                    .child(
                        svg()
                            .path("icons/pin.svg")
                            .size(px(13.))
                            .text_color(rgb(if pinned { style::RUN } else { style::MUTED })),
                    )
                    .tooltip(text(if pinned { "island.unpin" } else { "island.pin" }))
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .color(rgba(0).into())
                            .border(rgba(0).into())
                            .hover(rgb(0x1d1d1d).into())
                            .active(rgb(0x303030).into()),
                    )
                    .on_click(cx.listener(|view, _, _, cx| view.event(IslandEvent::TogglePin, cx))),
            );
        } else if mode == IslandMode::Alert {
            left = left
                .child(div().size(px(7.)).rounded(px(4.)).bg(rgb(style::ATTENTION)))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(style::ATTENTION))
                        .child(text("island.needs_you")),
                );
            right = right.child(
                div()
                    .text_color(rgb(style::FAINT))
                    .text_size(px(11.))
                    .truncate()
                    .child(
                        workspaces
                            .iter()
                            .find(|workspace| {
                                workspace.projects.iter().any(|project| {
                                    let selected = controller.displays.state.alert_id();
                                    project.approvals.iter().any(|a| {
                                        Some(super::controller::alert_key(
                                            project.source,
                                            &workspace.id,
                                            &project.area_id,
                                            &a.target,
                                        ))
                                        .as_deref()
                                            == selected
                                    }) || project.questions.iter().any(|q| {
                                        Some(super::controller::alert_key(
                                            project.source,
                                            &workspace.id,
                                            &project.area_id,
                                            &q.snapshot.question.target,
                                        ))
                                        .as_deref()
                                            == selected
                                    })
                                })
                            })
                            .map(|ws| ws.name.clone())
                            .unwrap_or_default(),
                    ),
            );
        } else {
            if running == 0 {
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
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(running.to_string()),
                    );
                let mut stack = div().flex();
                for (index, row) in workspaces
                    .iter()
                    .flat_map(|ws| &ws.projects)
                    .flat_map(|p| &p.overview.running)
                    .take(3)
                    .enumerate()
                {
                    stack = stack.child(
                        style::avatar(&row.name, &row.profile, 16.)
                            .border_2()
                            .border_color(rgb(0))
                            .when(index > 0, |s| s.ml(px(-6.))),
                    );
                }
                left = left.child(stack);
            }
            right = if pending > 0 {
                right.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .h(px(20.))
                        .px(px(8.))
                        .rounded(px(10.))
                        .bg(rgba(0xffa94d29))
                        .text_color(rgb(style::ATTENTION))
                        .text_size(px(11.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(div().size(px(6.)).rounded(px(3.)).bg(rgb(style::ATTENTION)))
                        .child(pending.to_string()),
                )
            } else {
                right.child(
                    ui::component::Icon::new(ui::component::IconName::Check)
                        .size(px(13.))
                        .text_color(rgb(style::OK)),
                )
            };
        }
        div()
            .id("island-header")
            .h(px(layout.header_height as f32))
            .flex_none()
            .px(px(12.))
            .text_size(px(12.))
            .flex()
            .items_center()
            .justify_between()
            .on_click(cx.listener(|view, _, window, cx| {
                view.focus_header(window);
                if view.controller.read(cx).displays.mode_on(view.display) != IslandMode::Expanded {
                    view.event(IslandEvent::Expand, cx);
                }
            }))
            .child(left)
            .child(div().w(px(layout.notch_width as f32)).flex_none())
            .child(right)
    }

    pub fn tabs(&self, running: usize, pending: usize, inset: f64, cx: &Context<Self>) -> Div {
        let tab = self.controller.read(cx).displays.state.tab();
        let mut tabs = div()
            .flex()
            .gap(px(2.))
            .mx(px(inset as f32))
            .mt(px(4.))
            .p(px(3.))
            .rounded(px(11.))
            .bg(rgb(0x101010))
            .flex_none();
        for (index, (target, key, count)) in [
            (IslandTab::Pending, "island.pending", pending),
            (IslandTab::Running, "island.running", running),
            (IslandTab::Staff, "island.staff", 0),
        ]
        .into_iter()
        .enumerate()
        {
            let selected = tab == target;
            let label = div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(12.5))
                .font_weight(if selected {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::NORMAL
                })
                .child(text(key))
                .when(count > 0, |v| {
                    v.child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(if target == IslandTab::Pending {
                                style::ATTENTION
                            } else {
                                style::FAINT
                            }))
                            .child(count.to_string()),
                    )
                });
            tabs = tabs.child(
                Button::new(gpui::SharedString::from(format!("island-tab-{index}")))
                    .h(px(30.))
                    .flex_1()
                    .rounded(px(8.))
                    .child(label)
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .color(rgb(if selected { 0x292929 } else { 0x101010 }).into())
                            .foreground(rgb(if selected { style::FG } else { style::MUTED }).into())
                            .border(rgba(0).into())
                            .hover(rgb(0x242424).into())
                            .active(rgb(0x333333).into()),
                    )
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.event(IslandEvent::SelectTab(target), cx)
                    })),
            );
        }
        tabs
    }

    pub fn staff_guide(&self, cx: &Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .py(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .size(px(44.))
                            .rounded(px(22.))
                            .bg(rgb(style::STAFF))
                            .flex_none(),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(3.))
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(text("island.staff")),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(style::MUTED))
                                    .child(text("island.staff_guide")),
                            ),
                    ),
            )
            .child(
                div().flex().justify_end().child(
                    style::action(
                        "island-enable-staff",
                        text("island.open_staff_settings"),
                        Tone::Primary,
                        cx,
                    )
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.controller
                            .update(cx, |state, cx| state.open_staff_settings(cx))
                    })),
                ),
            )
    }
}
