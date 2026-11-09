//! Read remembered grants off the foreground; every delivery still revalidates its live target.
use super::{
    actions::IslandTarget,
    controller::{alert_key, IslandController},
    IslandProject, IslandWorkspace,
};
use gpui::Context;
use norma::mobile::ApprovalSnapshot;
use runtime::island::approval_rules::approval_rule_key;
pub(super) use runtime::island::approval_rules::APPROVAL_RULE_PREFIX as PREFIX;
use std::path::PathBuf;

#[derive(Clone)]
pub(super) struct RememberedGrant {
    database: PathBuf,
    key: String,
}

impl RememberedGrant {
    pub fn new(
        workspace: &IslandWorkspace,
        project: &IslandProject,
        approval: &ApprovalSnapshot,
    ) -> Option<Self> {
        Some(Self {
            database: workspace.database.clone(),
            key: approval_rule_key(
                &workspace.id,
                &project.area_id,
                project.root.to_str()?,
                approval,
            )?,
        })
    }
}

impl IslandController {
    pub fn remember_grant(&mut self, grant: RememberedGrant, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    storage::SettingsStore::new(grant.database).set_string(&grant.key, "true")
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.message = Some(ui::i18n::LocalizedText::new(if result.is_ok() {
                    "island.allow_remembered"
                } else {
                    "island.allow_not_saved"
                }));
                if result.is_ok() {
                    this.checked_grants.clear();
                    this.check_remembered(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn check_remembered(&mut self, cx: &mut Context<Self>) {
        let mut work = Vec::new();
        let mut live = std::collections::BTreeSet::new();
        for workspace in &self.model.read(cx).workspaces {
            for project in &workspace.projects {
                for approval in &project.approvals {
                    let id = alert_key(
                        project.source,
                        &workspace.id,
                        &project.area_id,
                        &approval.target,
                    );
                    live.insert(id.clone());
                    if self.pending.contains(&id) || !self.checked_grants.insert(id) {
                        continue;
                    }
                    if let Some(grant) = RememberedGrant::new(workspace, project, approval) {
                        work.push((
                            IslandTarget::new(workspace, project),
                            approval.target.clone(),
                            grant,
                        ));
                    }
                }
            }
        }
        self.checked_grants.retain(|id| live.contains(id));
        if work.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let grants = cx
                .background_executor()
                .spawn(async move {
                    work.into_iter()
                        .filter(|(_, _, grant)| {
                            storage::SettingsStore::new(grant.database.clone())
                                .get_string(&grant.key)
                                .is_ok_and(|value| value.as_deref() == Some("true"))
                        })
                        .map(|(route, target, _)| (route, target))
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                for (route, target) in grants {
                    this.approve(route, target, true, None, cx);
                }
            });
        })
        .detach();
    }
}
