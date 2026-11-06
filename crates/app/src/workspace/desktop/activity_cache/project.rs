//! All database/file reads and activity projections run on the background executor.
use super::{
    capture::ActivityCapture,
    cards::{card_from_status, chat_agent},
    project_activity, ActivityInput, ActivityOutput, ActivityPresentation,
};
use crate::island::{IslandProject, IslandWorkspace};
use norma::mobile::{
    ActivityApprovalFact, DesktopChatProject, DesktopChatWorkspace, RunOverviewFile,
    RunOverviewProjectPresence, WorkspaceActivityFacts, WorkspaceActivityView,
};
use storage::{DesktopAreaStatusStore, DesktopWorkspaceStore};

pub(super) struct Projection {
    pub islands: storage::StorageResult<Vec<IslandWorkspace>>,
    pub current: Option<ActivityOutput>,
}

pub(super) fn project_workspaces(mut capture: ActivityCapture) -> Projection {
    let current = capture.current.take().map(project_activity);
    Projection {
        current,
        islands: project_islands(capture),
    }
}

fn project_islands(capture: ActivityCapture) -> storage::StorageResult<Vec<IslandWorkspace>> {
    let store = DesktopWorkspaceStore::new(capture.database.clone());
    let status_store = DesktopAreaStatusStore::new(capture.database.clone());
    let mut outputs = Vec::new();
    for workspace in store.list()? {
        let areas = store.list_projects(&workspace.id)?;
        let statuses = status_store.list(&workspace.id)?;
        let artifacts = super::super::artifacts::load_desktop_artifacts(
            &capture.database,
            &workspace.id,
            &areas,
        )?;
        let mut projects = Vec::new();
        let mut cached_projects = Vec::new();
        let mut presentation = Vec::new();
        let mut presence = Vec::new();
        let mut approvals = Vec::new();
        let mut questions = Vec::new();
        for area in &areas {
            let live = capture.live.get(&(workspace.id.clone(), area.id.clone()));
            let status = statuses.iter().find(|status| status.area_id == area.id);
            cached_projects.push(DesktopChatProject {
                area_id: area.id.clone(),
                project_id: area.id.clone(),
                name: area.display_name.clone(),
                root: area.root.to_string_lossy().into_owned(),
                definitions: Vec::new(),
                agents: status
                    .into_iter()
                    .flat_map(|status| &status.agents)
                    .map(|agent| chat_agent(agent.name.clone(), card_from_status(agent, None)))
                    .collect(),
            });
            let agents = if let Some(live) = live {
                approvals.extend(live.approvals.iter().map(|approval| ActivityApprovalFact {
                    area_id: area.id.clone(),
                    approval: approval.clone(),
                }));
                questions.extend(live.questions.clone());
                presentation.extend(live.agents.iter().map(|agent| agent.presentation.clone()));
                live.agents
                    .iter()
                    .map(|agent| chat_agent(agent.name.clone(), agent.card.clone()))
                    .collect()
            } else {
                status
                    .into_iter()
                    .flat_map(|status| &status.agents)
                    .map(|agent| {
                        presentation.push(ActivityPresentation {
                            area_id: area.id.clone(),
                            node_id: agent.node_id,
                            live: false,
                            updated_at_unix_ms: status.map(|s| s.updated_at_unix_ms),
                            ego_stop_agent: None,
                        });
                        chat_agent(agent.name.clone(), card_from_status(agent, None))
                    })
                    .collect()
            };
            presence.push(RunOverviewProjectPresence {
                area_id: area.id.clone(),
                loaded: live.is_some(),
                updated_at_unix_ms: status.map(|status| status.updated_at_unix_ms),
            });
            projects.push(DesktopChatProject {
                area_id: area.id.clone(),
                project_id: area.id.clone(),
                name: area.display_name.clone(),
                root: area.root.to_string_lossy().into_owned(),
                agents,
                definitions: Vec::new(),
            });
        }
        let files = artifact_files(&artifacts, &areas);
        outputs.push(project_activity(ActivityInput {
            database: capture.database.clone(),
            area_ids: areas.into_iter().map(|area| area.id).collect(),
            presentation,
            current_area_id: String::new(),
            files,
            presence,
            questions,
            cached_facts: Some(WorkspaceActivityFacts {
                workspace: DesktopChatWorkspace {
                    id: workspace.id.clone(),
                    name: workspace.name.clone(),
                    revision: 0,
                    projects: cached_projects,
                },
                observed: true,
                approvals: Vec::new(),
                reviews: Vec::new(),
            }),
            facts: WorkspaceActivityFacts {
                workspace: DesktopChatWorkspace {
                    id: workspace.id,
                    name: workspace.name,
                    revision: 0,
                    projects,
                },
                observed: true,
                approvals,
                reviews: Vec::new(),
            },
        }));
    }
    Ok(outputs.into_iter().map(|output| output.island).collect())
}

pub(super) fn island_workspace(
    input: &ActivityInput,
    view: &WorkspaceActivityView,
) -> IslandWorkspace {
    // The island is global: changing the focused area must not reorder or falsely load a project.
    let overview = norma::mobile::project_run_overview("", view, &input.files, &input.presence);
    let cached_view =
        WorkspaceActivityView::from_facts(input.cached_facts.as_ref().unwrap_or(&input.facts));
    let cached_presence: Vec<_> = input
        .presence
        .iter()
        .map(|presence| RunOverviewProjectPresence {
            loaded: false,
            ..presence.clone()
        })
        .collect();
    let cached =
        norma::mobile::project_run_overview("", &cached_view, &input.files, &cached_presence);
    let projects = input
        .facts
        .workspace
        .projects
        .iter()
        .map(|project| {
            let mut projected = overview.clone();
            projected
                .attention
                .retain(|row| row.area_id == project.area_id);
            projected
                .running
                .retain(|row| row.area_id == project.area_id);
            projected.idle.retain(|row| row.area_id == project.area_id);
            projected
                .unloaded_projects
                .retain(|row| row.area_id == project.area_id);
            let mut cached_overview = cached.clone();
            cached_overview
                .attention
                .retain(|row| row.area_id == project.area_id);
            cached_overview
                .unloaded_projects
                .retain(|row| row.area_id == project.area_id);
            IslandProject {
                source: 0,
                area_id: project.area_id.clone(),
                name: project.name.clone(),
                root: project.root.clone().into(),
                loaded: input
                    .presence
                    .iter()
                    .any(|presence| presence.area_id == project.area_id && presence.loaded),
                overview: projected,
                cached_overview,
                approvals: input
                    .facts
                    .approvals
                    .iter()
                    .filter(|fact| fact.area_id == project.area_id)
                    .map(|fact| fact.approval.clone())
                    .collect(),
                questions: input
                    .questions
                    .iter()
                    .filter(|question| question.area_id == project.area_id)
                    .cloned()
                    .collect(),
            }
        })
        .collect();
    IslandWorkspace {
        database: input.database.clone(),
        id: input.facts.workspace.id.clone(),
        name: input.facts.workspace.name.clone(),
        projects,
    }
}

pub(super) fn artifact_files(
    artifacts: &[crate::workspace::desktop::DesktopArtifact],
    areas: &[storage::DesktopProjectArea],
) -> Vec<RunOverviewFile> {
    artifacts
        .iter()
        .filter(|artifact| !artifact.viewed)
        .map(|artifact| RunOverviewFile {
            area_id: artifact.area_id.clone(),
            node_id: artifact.node_id,
            agent_hint: artifact.agent_profile.clone(),
            project_hint: areas
                .iter()
                .find(|area| area.id == artifact.area_id)
                .map(|area| area.display_name.clone())
                .unwrap_or_default(),
            path: artifact.source_path.clone(),
            turn_id: artifact.turn_id.clone(),
            fingerprint: artifact.version.clone(),
        })
        .collect()
}
