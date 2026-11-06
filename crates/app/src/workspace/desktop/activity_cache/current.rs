//! Keep the existing window overview on its memory cache until the island replaces it.
use super::super::status::{ego_stop_agent, live_agents};
use super::{
    cards::{card_from_status, chat_agent},
    ActivityInput, ActivityPresentation,
};
use crate::app::MultisoulApp;
use gpui::App;
use norma::mobile::{
    DesktopChatAgent, DesktopChatProject, DesktopChatWorkspace, RunOverviewFile,
    RunOverviewProjectPresence, WorkspaceActivityFacts,
};

impl MultisoulApp {
    pub(super) fn capture_current_activity_input(&self, cx: &App) -> Option<ActivityInput> {
        let workspace_id = self.desktop_workspace.active_id.clone()?;
        let workspace_name = self
            .desktop_workspace
            .active()
            .map(|workspace| workspace.name.clone())
            .unwrap_or_default();
        let mut projects = Vec::new();
        let mut presentation = Vec::new();
        let mut area_ids = Vec::new();
        for area in &self.desktop_workspace.areas {
            area_ids.push(area.id.clone());
            let (agents, presented) = self.capture_area_agents(&workspace_id, &area.id, cx);
            presentation.extend(presented);
            projects.push(DesktopChatProject {
                area_id: area.id.clone(),
                project_id: area.id.clone(),
                name: area.display_name.clone(),
                root: String::new(),
                agents,
                definitions: Vec::new(),
            });
        }
        let current_area_id = self
            .desktop_workspace
            .expanded_area_id
            .clone()
            .or_else(|| self.desktop_workspace.active_area_id.clone())
            .unwrap_or_default();
        let files = self
            .desktop_workspace
            .content_artifacts
            .iter()
            .filter(|artifact| !artifact.viewed)
            .map(|artifact| RunOverviewFile {
                area_id: artifact.area_id.clone(),
                node_id: artifact.node_id,
                agent_hint: artifact.agent_profile.clone(),
                project_hint: self
                    .desktop_workspace
                    .areas
                    .iter()
                    .find(|area| area.id == artifact.area_id)
                    .map(|area| area.display_name.clone())
                    .unwrap_or_default(),
                path: artifact.source_path.clone(),
                turn_id: artifact.turn_id.clone(),
                fingerprint: artifact.version.clone(),
            })
            .collect();
        let presence = self
            .desktop_workspace
            .areas
            .iter()
            .map(|area| RunOverviewProjectPresence {
                area_id: area.id.clone(),
                loaded: self
                    .desktop_workspace
                    .sessions
                    .contains_key(&(workspace_id.clone(), area.id.clone())),
                updated_at_unix_ms: self
                    .desktop_workspace
                    .status_cache
                    .get(&area.id)
                    .map(|status| status.updated_at_unix_ms),
            })
            .collect();
        Some(ActivityInput {
            database: self.database_paths.app_db_path().to_owned(),
            cached_facts: None,
            questions: self.brain_overview().questions.clone(),
            area_ids,
            presentation,
            current_area_id,
            files,
            presence,
            facts: WorkspaceActivityFacts {
                workspace: DesktopChatWorkspace {
                    id: workspace_id,
                    name: workspace_name,
                    revision: 0,
                    projects,
                },
                observed: true,
                approvals: self
                    .brain_overview()
                    .approvals
                    .iter()
                    .map(|(area, approval)| norma::mobile::ActivityApprovalFact {
                        area_id: area.clone(),
                        approval: approval.clone(),
                    })
                    .collect(),
                reviews: Vec::new(),
            },
        })
    }

    fn capture_area_agents(
        &self,
        workspace_id: &str,
        area_id: &str,
        cx: &App,
    ) -> (Vec<DesktopChatAgent>, Vec<ActivityPresentation>) {
        let view = self.desktop_area_status_view(workspace_id, area_id, cx);
        let mut agents = Vec::new();
        let mut presentation = Vec::new();
        if view.live {
            if let Some(session) = self
                .desktop_workspace
                .sessions
                .get(&(workspace_id.to_owned(), area_id.to_owned()))
            {
                let project = session.read(cx);
                let snapshots = project.mobile_card_snapshots();
                for agent in live_agents(project) {
                    let card = card_from_status(
                        &agent,
                        snapshots.iter().find(|card| card.node_id == agent.node_id),
                    );
                    presentation.push(ActivityPresentation {
                        area_id: area_id.to_owned(),
                        node_id: agent.node_id,
                        live: true,
                        updated_at_unix_ms: None,
                        ego_stop_agent: agent
                            .state
                            .starts_with("ego_")
                            .then(|| ego_stop_agent(project, agent.node_id))
                            .flatten(),
                    });
                    agents.push(chat_agent(agent.name, card));
                }
                return (agents, presentation);
            }
        }
        for agent in view.agents {
            let node_id = agent.node_id;
            let name = agent.name.clone();
            presentation.push(ActivityPresentation {
                area_id: area_id.to_owned(),
                node_id,
                live: false,
                updated_at_unix_ms: view.updated_at_unix_ms,
                ego_stop_agent: None,
            });
            agents.push(chat_agent(name, card_from_status(&agent, None)));
        }
        (agents, presentation)
    }
}
