//! While the island is hidden, requests that need the user are mirrored as system notifications.
use super::{
    actions::IslandTarget,
    controller::{alert_key, IslandController},
};
use gpui::Context;
use runtime::island::{IslandAlert, IslandAlertKind};
use ui::i18n::{message, text};

trait Request {
    fn key(&self, project: &super::IslandProject, workspace: &super::IslandWorkspace) -> String;
}
impl<T: serde::Serialize> Request for T {
    fn key(&self, project: &super::IslandProject, workspace: &super::IslandWorkspace) -> String {
        alert_key(project.source, &workspace.id, &project.area_id, self)
    }
}

pub(super) type Post = Box<dyn Fn(&str, &str, &str)>;

const COMPLETION_ID: &str = "island.completed";

impl IslandController {
    #[cfg(target_os = "macos")]
    pub(super) fn install_notifier(&mut self, cx: &mut Context<Self>) {
        let (clicks, requests) = async_channel::unbounded();
        let (permission, outcomes) = async_channel::unbounded();
        let Some(notifier) = super::notify::Notifier::new(clicks, permission) else {
            return;
        };
        cx.spawn(async move |_, cx| {
            while let Ok((granted, failed)) = outcomes.recv().await {
                let _ = cx.update(|cx| {
                    super::preferences::report_notification_permission(granted, failed, cx)
                });
            }
        })
        .detach();
        self.post = Some(Box::new(move |id, title, body| {
            notifier.post(id, title, body)
        }));
        cx.spawn(async move |this, cx| {
            while let Ok(id) = requests.recv().await {
                if this
                    .update(cx, |this, cx| this.notification_clicked(&id, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn current_alerts(&self, cx: &gpui::App) -> Vec<IslandAlert> {
        let mut alerts = Vec::new();
        for workspace in &self.model.read(cx).workspaces {
            for project in &workspace.projects {
                let key = |request: &dyn Request| request.key(project, workspace);
                alerts.extend(project.approvals.iter().map(|a| IslandAlert {
                    id: key(&a.target),
                    kind: IslandAlertKind::Approval,
                }));
                alerts.extend(project.questions.iter().map(|q| IslandAlert {
                    id: key(&q.snapshot.question.target),
                    kind: IslandAlertKind::Question,
                }));
            }
        }
        alerts
    }

    /// Mirrors new approvals/questions to the system when the island is hidden.
    pub(super) fn notify_hidden(&mut self, cx: &Context<Self>) {
        let alerts = self.current_alerts(cx);
        let fresh = self.gate.fresh(super::preferences::current(cx), &alerts);
        let Some(post) = &self.post else { return };
        for alert in fresh {
            let Some((_, _, workspace)) = self.alert_site(&alert.id, cx) else {
                continue;
            };
            let title = text(match alert.kind {
                IslandAlertKind::Approval => "island.notify_approval",
                IslandAlertKind::Question => "island.notify_question",
            });
            let body = message("island.notify_body", &[("workspace", workspace)]);
            post(&alert.id, title, &body.to_string());
        }
    }

    pub(super) fn notify_completed(&self, label: &str, cx: &gpui::App) {
        let preferences = super::preferences::current(cx);
        if let (true, true, Some(post)) = (preferences.hidden, preferences.completions, &self.post)
        {
            post(COMPLETION_ID, text("island.notify_completed"), label);
        }
    }

    /// Resolves a notification identifier to the exact card it was raised for.
    fn alert_site(&self, id: &str, cx: &gpui::App) -> Option<(IslandTarget, u64, String)> {
        for workspace in &self.model.read(cx).workspaces {
            for project in &workspace.projects {
                let key = |request: &dyn Request| request.key(project, workspace);
                let node = project
                    .approvals
                    .iter()
                    .find(|a| key(&a.target) == id)
                    .map(|a| a.target.node_id)
                    .or_else(|| {
                        project
                            .questions
                            .iter()
                            .find(|q| key(&q.snapshot.question.target) == id)
                            .map(|q| q.snapshot.question.target.node_id)
                    });
                if let Some(node) = node {
                    return Some((
                        IslandTarget::new(workspace, project),
                        node,
                        workspace.name.clone(),
                    ));
                }
            }
        }
        None
    }

    /// Clicking a notification brings Norma forward and focuses the original card.
    pub(super) fn notification_clicked(&mut self, id: &str, cx: &mut Context<Self>) {
        cx.activate(true);
        if let Some((target, node, _)) = self.alert_site(id, cx) {
            self.locate(target, node, cx);
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/island_fallback_tests.rs"]
mod tests;
