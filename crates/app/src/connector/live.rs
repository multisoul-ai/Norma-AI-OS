//! Foreground capture only. No SQLite, PTYs, session loading, conversation or terminal reads.
use super::repository::LiveRequest;
use gpui::App;
use norma::connector::{Card, CardIdentity, CardSigner};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::Path;

pub(super) fn capture(database: &Path, mut call: LiveRequest, signer: &CardSigner, cx: &mut App) {
    let active = crate::workspace::desktop::sessions::hosts(database, cx)
        .iter()
        .filter_map(|host| host.read(cx).desktop_workspace.active_id.clone())
        .collect::<std::collections::HashSet<_>>();
    for workspace in &mut call.snapshot.workspaces {
        workspace.active = active.contains(&workspace.workspace_id);
    }
    for area in &call.areas {
        let Some(session) = crate::workspace::desktop::mobile::session(database, area, cx) else {
            continue;
        };
        let approvals_by_node = session.update(cx, |app, _| {
            app.mobile_card_snapshots()
                .into_iter()
                .map(|card| {
                    let approvals = app
                        .runtime
                        .mobile_approval_snapshots(card.node_id)
                        .unwrap_or_default();
                    (card.node_id, approvals)
                })
                .collect::<std::collections::HashMap<_, _>>()
        });
        let app = session.read(cx);
        if let Some(coordinator) = &app.agent_task_coordinator {
            for task in &mut call.snapshot.tasks {
                if !task.terminal()
                    && task.workspace_id == area.desktop_workspace_id
                    && signer.verify(&task.card_id).is_ok_and(|identity| {
                        identity.workspace_id == area.desktop_workspace_id
                            && identity.area_id == area.id
                    })
                {
                    if let Ok(actual) = coordinator.task_by_ref(&task.task_id) {
                        task.state = actual.state.into();
                        if app
                            .pending_agent_submissions
                            .task_stage(actual.task_id)
                            .is_some()
                            && matches!(actual.state, "queued" | "running" | "cancel_requested")
                        {
                            task.state = "queued".into();
                        }
                        if let Some(result) = actual.result {
                            task.result_summary =
                                result.text.map(|text| text.chars().take(500).collect());
                        }
                    }
                }
            }
        }
        let Some(workspace) = call
            .snapshot
            .workspaces
            .iter_mut()
            .find(|w| w.workspace_id == area.desktop_workspace_id)
        else {
            continue;
        };
        let previous = workspace
            .cards
            .iter()
            .filter(|c| c.identity.area_id == area.id)
            .cloned()
            .collect::<Vec<_>>();
        workspace.cards.retain(|c| c.identity.area_id != area.id);
        let project = workspace
            .projects
            .iter()
            .find(|p| p.area_id == area.id)
            .expect("project");
        let canvas_id = hex::encode(Sha256::digest(
            format!("{}:{}", area.desktop_workspace_id, area.id).as_bytes(),
        ));
        for card in app.mobile_card_snapshots() {
            let Some(node) = app.runtime.document.node(card.node_id) else {
                continue;
            };
            let canvas::CanvasNodeKind::Agent(spec) = &node.kind else {
                continue;
            };
            let definition = app
                .agent_definitions
                .definitions()
                .iter()
                .find(|d| Some(d.id) == spec.definition_id);
            let definition_id = spec.definition_id.unwrap_or(0);
            let role_name = definition
                .map(|d| d.name.clone())
                .unwrap_or_else(|| node.kind.display_title());
            let identity = previous
                .iter()
                .find(|c| {
                    c.identity.node_id == card.node_id && c.identity.definition_id == definition_id
                })
                .map(|c| c.identity.clone())
                .unwrap_or(CardIdentity {
                    workspace_id: area.desktop_workspace_id.clone(),
                    area_id: area.id.clone(),
                    canvas_id: canvas_id.clone(),
                    node_id: card.node_id,
                    definition_id,
                    created_at: call
                        .births
                        .get(&(
                            area.desktop_workspace_id.clone(),
                            area.id.clone(),
                            definition_id,
                        ))
                        .copied()
                        .unwrap_or(area.added_at_unix_ms / 1000),
                });
            let id = signer.sign(&identity);
            let approvals = approvals_by_node
                .get(&card.node_id)
                .cloned()
                .unwrap_or_default();
            let active_index = call
                .snapshot
                .tasks
                .iter()
                .rposition(|t| {
                    !t.terminal()
                        && t.card_id == id
                        && !matches!(t.state.as_str(), "queued" | "accepted")
                })
                .or_else(|| {
                    call.snapshot
                        .tasks
                        .iter()
                        .rposition(|t| !t.terminal() && t.card_id == id)
                });
            let active_task = active_index.map(|index| &mut call.snapshot.tasks[index]);
            let mut current = None;
            if let Some(task) = active_task {
                current = Some(task.task_id.clone());
                if !matches!(task.state.as_str(), "accepted" | "queued") {
                    task.progress = card.task_progress.clone();
                }
                task.pending_approval=approvals.first().map(|a|json!({"approval_id":format!("a1.{}",hex::encode(Sha256::digest(serde_json::to_vec(&a.target).expect("approval target")))),"tool":a.tool,"summary":"此卡片有待审批的工具调用"}));
                if let Some(summary) = card.task_summary.as_ref() {
                    task.summary = summary.chars().take(500).collect();
                }
            }
            let state = if !approvals.is_empty() {
                "waiting_approval"
            } else if current.is_some()
                || matches!(
                    card.state.as_str(),
                    "thinking" | "running_tool" | "preparing" | "interrupting"
                )
            {
                "busy"
            } else if matches!(card.state.as_str(), "ready" | "done" | "ended" | "idle") {
                "idle"
            } else {
                "unknown"
            };
            let revision = card_revision(app, &identity);
            workspace.cards.push(Card {
                identity,
                card_id: id,
                card_revision: revision,
                role_name,
                role_name_unique: false,
                project_id: project.project_id.clone(),
                profile: card.profile,
                state: state.into(),
                current_task_id: current,
                loaded: true,
            });
        }
    }
    let _ = call.reply.try_send(Ok(call.snapshot));
}

/// Logical route and Provider session fingerprint. Registration UUIDs and PTY run IDs
/// are process lifetimes, not persisted target revisions; reopening the same session
/// must not invalidate a connector target. All reads below are in-memory.
fn card_revision(app: &crate::app::MultisoulApp, identity: &CardIdentity) -> String {
    let canvas::CanvasNodeKind::Agent(spec) = &app
        .runtime
        .document
        .node(identity.node_id)
        .expect("card")
        .kind
    else {
        unreachable!("agent card")
    };
    hex::encode(Sha256::digest(
        serde_json::to_vec(&json!([
            "r1",
            identity,
            app.runtime
                .workspace_root()
                .map(|path| path.to_string_lossy()),
            app.runtime
                .agent_worktree(identity.node_id)
                .map(|path| path.to_string_lossy()),
            spec.profile,
            spec.definition_id,
            spec.mobile_control,
            app.runtime.agent_session_id(identity.node_id)
        ]))
        .expect("revision"),
    ))
}

#[cfg(test)]
#[path = "../../tests/unit/connector_revision_tests.rs"]
mod connector_revision_tests;
