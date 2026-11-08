use super::{
    actions::IslandTarget,
    controller::{alert_key, IslandController},
    style::{self, Tone},
    view::IslandView,
    IslandProject, IslandWorkspace,
};
use gpui::{div, prelude::*, px, rgb, AnyElement, Context};
use norma::mobile::ApprovalTarget;
use ui::{component::Disableable, i18n::text};

impl IslandController {
    pub fn approve(
        &mut self,
        route: IslandTarget,
        target: ApprovalTarget,
        allow: bool,
        remember: Option<super::remembered::RememberedGrant>,
        cx: &mut Context<Self>,
    ) {
        let id = alert_key(route.source, &route.workspace, &route.area, &target);
        if self.pending.contains(&id) {
            return;
        }
        let Some(host) = route.host(cx) else {
            self.message = Some(ui::i18n::LocalizedText::new("island.expired"));
            cx.notify();
            return;
        };
        let result = host.update(cx, |app, cx| {
            app.decide_overview_approval_at(&route.workspace, &route.area, &target, allow, cx)
        });
        let receipt = match result {
            Ok(receipt) => receipt,
            Err(_) => {
                self.message = Some(ui::i18n::LocalizedText::new("island.expired"));
                cx.notify();
                return;
            }
        };
        self.pending.insert(id.clone());
        self.message = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = receipt.recv().await;
            let _ = this.update(cx, |this, cx| {
                this.pending.remove(&id);
                let confirmed = matches!(&result, Ok(Ok(value)) if value["status"] == "submitted" || value["status"] == "responded");
                this.message = Some(ui::i18n::LocalizedText::new(if confirmed { "island.submitted" } else { "island.unconfirmed" }));
                this.action_feedback(id, confirmed, cx);
                if confirmed { if let Some(grant) = remember { this.remember_grant(grant, cx); } }
                if let Some(host) = route.host(cx) { host.update(cx, |app, cx| app.note_workspace_activity_changed(cx)); }
                cx.notify();
            });
        }).detach();
    }
}

impl IslandView {
    pub fn render_pending(
        &mut self,
        workspace: &IslandWorkspace,
        project: &IslandProject,
        alert_only: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let route = IslandTarget::new(workspace, project);
        let selected = self
            .controller
            .read(cx)
            .displays
            .state
            .alert_id()
            .map(str::to_owned);
        let mut rows = Vec::new();
        for approval in &project.approvals {
            let id = alert_key(
                route.source,
                &route.workspace,
                &route.area,
                &approval.target,
            );
            if alert_only && selected.as_deref() != Some(&id) {
                continue;
            }
            if approval.decisions.is_empty() {
                rows.push(
                    style::card()
                        .child(style::card_heading(
                            &approval.card_title,
                            &approval.target.profile,
                            format!(
                                "{} · {} · {}",
                                workspace.name, approval.target.profile, project.name
                            ),
                            text("island.approval_title"),
                            text("island.approval_label"),
                            style::ATTENTION,
                            runtime::island::bloub::Mood::Waiting,
                        ))
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(rgb(style::MUTED))
                                .child(text("island.native_approval")),
                        )
                        .child(div().flex().justify_end().child(self.locate_button(
                            &route,
                            approval.target.node_id,
                            cx,
                        )))
                        .into_any_element(),
                );
                continue;
            }
            let waiting = self.controller.read(cx).pending.contains(&id);
            let mut actions = div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(if alert_only {
                    self.all_pending_button(cx)
                } else {
                    self.locate_button(&route, approval.target.node_id, cx)
                })
                .child(div().flex_1());
            let grant = super::remembered::RememberedGrant::new(workspace, project, approval);
            for (allow, remember, key) in [
                (false, false, "island.deny"),
                (true, true, "island.allow_always"),
                (true, false, "island.allow_once"),
            ] {
                if alert_only && remember {
                    continue;
                }
                let route = route.clone();
                let target = approval.target.clone();
                let grant = if remember { grant.clone() } else { None };
                actions = actions.child(
                    style::action(
                        format!("{id}-{key}"),
                        text(key),
                        if !allow {
                            Tone::Danger
                        } else if remember {
                            Tone::Neutral
                        } else {
                            Tone::Primary
                        },
                        cx,
                    )
                    .disabled(
                        waiting
                            || (remember && grant.is_none())
                            || !approval.decisions.contains(&if allow {
                                norma::agent_hook::ApprovalDecision::Allow
                            } else {
                                norma::agent_hook::ApprovalDecision::Deny
                            }),
                    )
                    .when(remember, |button| {
                        button.tooltip(text(if grant.is_some() {
                            "island.allow_always_hint"
                        } else {
                            "island.allow_not_supported"
                        }))
                    })
                    .on_click(cx.listener(move |view, _, _, cx| {
                        cx.stop_propagation();
                        view.controller.update(cx, |controller, cx| {
                            controller.approve(
                                route.clone(),
                                target.clone(),
                                allow,
                                grant.clone(),
                                cx,
                            )
                        });
                    })),
                );
            }
            let details = approval
                .details
                .get("command")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| approval.details.to_string());
            rows.push(
                style::card()
                    .child(style::card_heading(
                        &approval.card_title,
                        &approval.target.profile,
                        format!(
                            "{} · {} · {}",
                            workspace.name, approval.target.profile, project.name
                        ),
                        text("island.approval_title"),
                        text("island.approval_label"),
                        style::ATTENTION,
                        runtime::island::bloub::Mood::Waiting,
                    ))
                    .child(
                        div()
                            .max_h(px(120.))
                            .id(gpui::SharedString::from(format!("{id}-details")))
                            .overflow_y_scroll()
                            .px(px(10.))
                            .py(px(8.))
                            .rounded(px(9.))
                            .bg(rgb(0x070707))
                            .border_1()
                            .border_color(rgb(0x1d1d1d))
                            .font_family("Menlo")
                            .text_size(px(12.))
                            .child(format!("$ {details}")),
                    )
                    .when_some(
                        approval
                            .details
                            .get("description")
                            .and_then(|v| v.as_str())
                            .map(str::to_owned),
                        |card, why| {
                            card.child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(style::MUTED))
                                    .child(why),
                            )
                        },
                    )
                    .child(actions)
                    .into_any_element(),
            );
        }
        for row in &project.questions {
            let id = alert_key(
                route.source,
                &route.workspace,
                &route.area,
                &row.snapshot.question.target,
            );
            if alert_only && selected.as_deref() != Some(&id) {
                continue;
            }
            rows.push(self.render_question(
                route.clone(),
                row,
                id,
                format!(
                    "{} · {:?} · {}",
                    workspace.name, row.snapshot.question.target.provider, project.name
                ),
                cx,
            ));
        }
        if !alert_only {
            rows.extend(self.render_artifacts(workspace, project, None, cx));
        }
        rows
    }

    fn all_pending_button(&self, cx: &Context<Self>) -> ui::component::button::Button {
        let (_, count) =
            super::controller::counts(&self.controller.read(cx).visible_workspaces(cx));
        style::action(
            "island-all-pending",
            ui::i18n::format("island.all_pending", &[("count", count.to_string())]),
            Tone::Ghost,
            cx,
        )
        .on_click(cx.listener(|view, _, _, cx| {
            view.event(runtime::island::IslandEvent::Expand, cx);
            view.event(
                runtime::island::IslandEvent::SelectTab(runtime::island::IslandTab::Pending),
                cx,
            );
        }))
    }
}
