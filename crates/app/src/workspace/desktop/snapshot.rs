//! Read-only status projection for Norma's workspace_snapshot tool.

use crate::app::{AppScreen, MultisoulApp};
use agent::{AdapterError, AdapterResult, WorkspaceSnapshotScope};
use gpui::{App, Context, EntityId};
use serde_json::{json, Value};
use storage::DesktopAgentStatus;

impl MultisoulApp {
    pub(crate) fn norma_workspace_snapshot(
        &mut self,
        scope: WorkspaceSnapshotScope,
        area_id: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Result<AdapterResult, AdapterError> {
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            // The source project is currently leased by the Adapter update. Pass its live
            // projection to the host so a workspace-wide query never reads that entity again.
            let source = (cx.entity().entity_id(), super::status::live_agents(self));
            return host.update(cx, |host, cx| {
                host.norma_workspace_snapshot_on_host(scope, area_id, Some(source), cx)
            });
        }
        self.norma_workspace_snapshot_on_host(scope, area_id, None, cx)
    }

    fn norma_workspace_snapshot_on_host(
        &self,
        scope: WorkspaceSnapshotScope,
        area_id: Option<&str>,
        source: Option<(EntityId, Vec<DesktopAgentStatus>)>,
        cx: &App,
    ) -> Result<AdapterResult, AdapterError> {
        if self.screen != AppScreen::DesktopWorkspace {
            return Err(AdapterError::Rejected("请先打开桌面工作空间".into()));
        }
        let workspace = self
            .desktop_workspace
            .active()
            .ok_or_else(|| AdapterError::Rejected("当前没有打开的桌面工作空间".into()))?;
        let focused = self
            .desktop_workspace
            .expanded_area_id
            .as_deref()
            .filter(|id| {
                self.desktop_workspace
                    .sessions
                    .contains_key(&(workspace.id.clone(), (*id).to_owned()))
            });
        let selected = match scope {
            WorkspaceSnapshotScope::Workspace => None,
            WorkspaceSnapshotScope::CurrentProject => focused,
            WorkspaceSnapshotScope::Area => {
                Some(area_id.ok_or_else(|| AdapterError::InvalidArguments("缺少 area_id".into()))?)
            }
        };
        let mut projects = Vec::new();
        for area in &self.desktop_workspace.areas {
            if selected.is_some_and(|target| target != area.id) {
                continue;
            }
            let status = match source.as_ref() {
                Some((source_id, agents))
                    if self
                        .desktop_workspace
                        .sessions
                        .get(&(workspace.id.clone(), area.id.clone()))
                        .is_some_and(|session| session.entity_id() == *source_id) =>
                {
                    super::status::DesktopAreaStatusView {
                        agents: agents.clone(),
                        live: true,
                        updated_at_unix_ms: None,
                    }
                }
                _ => self.desktop_area_status_view(&workspace.id, &area.id, cx),
            };
            let live = status.live;
            let agents = status
                .agents
                .into_iter()
                .map(|agent| {
                    json!({
                        "node_id": agent.node_id,
                        "name": agent.name,
                        "task": agent.task,
                        "state": agent.state,
                        "needs_approval": super::status::approval_is_pending(&agent.state),
                        "can_locate_approval": super::status::approval_is_actionable(
                            live,
                            &agent.state,
                            agent.run_id.as_deref(),
                            agent.turn_id.as_deref(),
                        ),
                        "run_id": agent.run_id,
                        "turn_id": agent.turn_id,
                    })
                })
                .collect::<Vec<_>>();
            let artifacts = self
                .desktop_workspace
                .content_artifacts
                .iter()
                .filter(|artifact| artifact.area_id == area.id)
                .map(|artifact| {
                    json!({
                        "node_id": artifact.node_id,
                        "turn_id": artifact.turn_id,
                        "path": artifact.path,
                        "version": artifact.version,
                        "source_path": artifact.source_path,
                    })
                })
                .collect::<Vec<_>>();
            projects.push(json!({
                "area_id": area.id,
                "project_name": area.display_name,
                "live": status.live,
                "updated_at_unix_ms": status.updated_at_unix_ms,
                "agents": agents,
                "artifacts": artifacts,
            }));
        }
        if selected.is_some() && projects.is_empty() {
            return Err(AdapterError::Rejected("目标项目不在当前工作空间".into()));
        }
        let snapshot: Value = json!({
            "workspace_id": workspace.id,
            "workspace_name": workspace.name,
            "scope": if selected.is_some() { "area" } else { "workspace" },
            "focused_area_id": focused,
            "projects": projects,
        });
        Ok(AdapterResult::succeeded("已读取当前工作空间的真实状态").with_snapshot(snapshot))
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/desktop_status_cache_tests.rs"]
mod status_cache_tests;
#[cfg(test)]
#[path = "../../../tests/unit/desktop_snapshot_tests.rs"]
mod tests;
