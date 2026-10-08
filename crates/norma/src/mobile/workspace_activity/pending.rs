use super::{
    excerpt, inbox_item_key, WorkspaceActivityFacts, WorkspaceActivitySummary,
    WorkspaceActivityTarget,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct InboxPendingItem {
    pub machine_id: String,
    /// Project-scoped identity shared by deduplication and dismissal on this Mac.
    pub item_key: String,
    pub kind: String,
    pub title: String,
    /// Unix seconds. Zero explicitly denotes missing timing; never replaced by refresh time.
    pub created_at: u64,
    pub desktop_workspace_id: String,
    pub workspace_name: String,
    pub project_id: String,
    pub project: String,
    pub area_id: String,
    /// Provider conversation identity: Codex thread_id / Claude session_id.
    pub thread_id: String,
    pub agent: String,
    pub reason: String,
    pub target: WorkspaceActivityTarget,
}

/// Complete list across authorized desktop facts; no project or agent display budget applies.
/// Matching comes from the same unbounded attention projection as the summary. The caller reads
/// project-scoped dismissal keys on a background worker before invoking this pure function.
pub fn pending_items(
    machine: &str,
    mut workspaces: Vec<WorkspaceActivityFacts>,
    dismissed: &HashSet<String>,
) -> Vec<InboxPendingItem> {
    // Pick a stable authorized binding when a project appears in multiple desktop workspaces.
    workspaces.sort_by(|a, b| a.workspace.id.cmp(&b.workspace.id));
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for facts in workspaces {
        let summary = WorkspaceActivitySummary::from_facts_unbounded(machine, facts.clone(), 0);
        for attention in summary.attention {
            let target = &attention.target;
            let Some(project) = facts
                .workspace
                .projects
                .iter()
                .find(|p| p.area_id == target.area_id)
            else {
                continue;
            };
            let item_key = inbox_item_key(&attention.kind, &project.project_id, target);
            if dismissed.contains(&item_key) || !seen.insert(item_key.clone()) {
                continue;
            }
            let card = project
                .agents
                .iter()
                .find(|a| a.card.node_id == target.node_id && a.card.profile == target.profile)
                .map(|a| &a.card);
            let timing = card.and_then(|c| c.task_timing);
            let created_at = match attention.kind.as_str() {
                "review" => facts
                    .reviews
                    .iter()
                    .find(|r| {
                        r.area_id == target.area_id
                            && r.is_pending()
                            && r.node_id == target.node_id
                            && r.profile == target.profile
                            && r.run_id == target.run_id
                            && r.session_id == target.session_id
                            && r.turn_id == target.turn_id
                            && Some(&r.fingerprint) == target.fingerprint.as_ref()
                    })
                    .map_or(0, |r| r.saved_at_unix),
                "failed" => timing.map_or(0, |t| {
                    t.started_at_unix_ms
                        .saturating_add(t.completed_elapsed_ms.unwrap_or(0))
                        / 1000
                }),
                // Approval snapshots/audits have no request wall clock. Use the real turn start.
                _ => timing.map_or(0, |t| t.started_at_unix_ms / 1000),
            };
            let title = if attention.kind == "review" {
                // The live card may already describe a later turn; don't label old deliveries with it.
                format!("{} 的成果", attention.agent)
            } else {
                card.and_then(|c| c.task_title.as_deref())
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| excerpt(s, 160))
                    .unwrap_or_else(|| {
                        if attention.kind == "approval" {
                            let tool = facts
                                .approvals
                                .iter()
                                .find(|a| {
                                    a.area_id == target.area_id
                                        && Some(&a.approval.target.request_id)
                                            == target.request_id.as_ref()
                                })
                                .map_or("工具", |a| a.approval.tool.as_str());
                            format!("{} 请求运行 {}", attention.agent, excerpt(tool, 60))
                        } else {
                            format!("{} 的任务失败", attention.agent)
                        }
                    })
            };
            items.push(InboxPendingItem {
                machine_id: machine.into(),
                item_key,
                kind: attention.kind,
                title,
                created_at,
                desktop_workspace_id: facts.workspace.id.clone(),
                workspace_name: facts.workspace.name.clone(),
                project_id: project.project_id.clone(),
                project: attention.project,
                area_id: target.area_id.clone(),
                thread_id: target.session_id.clone(),
                agent: attention.agent,
                reason: attention.reason,
                target: attention.target,
            });
        }
    }
    items.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| a.workspace_name.cmp(&b.workspace_name))
            .then_with(|| a.item_key.cmp(&b.item_key))
    });
    items
}
