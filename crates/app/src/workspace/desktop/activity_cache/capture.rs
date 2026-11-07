//! Foreground capture is memory-only, including sessions retained without a window.
use super::{cards::card_from_status, ActivityPresentation};
use crate::app::{MultisoulApp, OverviewQuestion};
use crate::workspace::desktop::status::{ego_stop_agent, live_agents};
use gpui::{App, Context};
use norma::mobile::{ApprovalSnapshot, CardSnapshot};
use std::{collections::BTreeMap, path::PathBuf};

#[cfg(test)]
#[path = "../../../../tests/unit/island_native_approval_tests.rs"]
mod native_approval_tests;

#[derive(Clone, Debug)]
pub(super) struct LiveAgent {
    pub name: String,
    pub card: CardSnapshot,
    pub presentation: ActivityPresentation,
}

impl PartialEq for LiveAgent {
    fn eq(&self, other: &Self) -> bool {
        // CardSnapshot's protocol equality intentionally omits these display fields.
        self.name == other.name
            && self.card == other.card
            && self.card.latest_prompt == other.card.latest_prompt
            && self.card.task_timing == other.card.task_timing
            && self.card.task_progress == other.card.task_progress
            && self.card.task_summary == other.card.task_summary
            && self.presentation == other.presentation
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct LiveProject {
    pub agents: Vec<LiveAgent>,
    pub approvals: Vec<ApprovalSnapshot>,
    pub questions: Vec<OverviewQuestion>,
}

#[derive(Default)]
pub(crate) struct ActivityMonitor {
    live: BTreeMap<(String, String), LiveProject>,
    revision: u64,
    pub(super) registered: bool,
    pub(super) stopped: bool,
}

pub(super) struct ActivityCapture {
    pub generation: u64,
    pub database: PathBuf,
    pub current: Option<super::ActivityInput>,
    pub live: BTreeMap<(String, String), LiveProject>,
}

impl MultisoulApp {
    pub(super) fn capture_activity_input(&self, cx: &mut App) -> ActivityCapture {
        ActivityCapture {
            generation: self.desktop_workspace.activity_generation,
            database: self.database_paths.app_db_path().to_owned(),
            current: self.capture_current_activity_input(cx),
            live: self.capture_activity_sessions(cx),
        }
    }

    /// Activity capture survives window detachment without running any window UI refreshes.
    pub(super) fn start_island_activity_monitor(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(500))
                .await;
            let retained = this
                .update(cx, |app, cx| {
                    if app.desktop_workspace.activity_monitor.stopped {
                        return false;
                    }
                    app.refresh_island_activity_if_changed(cx);
                    true
                })
                .unwrap_or(false);
            if !retained {
                break;
            }
        })
        .detach();
    }

    fn refresh_island_activity_if_changed(&mut self, cx: &mut Context<Self>) {
        if self.desktop_workspace.activity_monitor.stopped || self.desktop_host.is_some() {
            return;
        }
        let live = self.capture_activity_sessions(cx);
        let revision = crate::mobile::project_events::revision();
        let monitor = &mut self.desktop_workspace.activity_monitor;
        if live != monitor.live || revision != monitor.revision {
            monitor.live = live;
            monitor.revision = revision;
            self.mark_workspace_activity_dirty(cx);
        }
    }

    pub(in crate::workspace::desktop) fn stop_island_activity(&mut self, cx: &mut Context<Self>) {
        self.desktop_workspace.activity_monitor.stopped = true;
        self.invalidate_workspace_activity();
        crate::island::remove_source(cx.entity().entity_id().as_u64(), cx);
    }

    fn capture_activity_sessions(&self, cx: &mut App) -> BTreeMap<(String, String), LiveProject> {
        self.desktop_workspace
            .sessions
            .iter()
            .map(|(key, session)| {
                let project = session.update(cx, |app, _| {
                    let snapshots = app.mobile_card_snapshots();
                    let agents = live_agents(app)
                        .into_iter()
                        .map(|agent| LiveAgent {
                            name: agent.name.clone(),
                            card: card_from_status(
                                &agent,
                                snapshots.iter().find(|c| c.node_id == agent.node_id),
                            ),
                            presentation: ActivityPresentation {
                                area_id: key.1.clone(),
                                node_id: agent.node_id,
                                live: true,
                                updated_at_unix_ms: None,
                                ego_stop_agent: agent
                                    .state
                                    .starts_with("ego_")
                                    .then(|| ego_stop_agent(app, agent.node_id))
                                    .flatten(),
                            },
                        })
                        .collect();
                    let approvals = snapshots
                        .iter()
                        .filter(|card| card.state == "waiting_approval")
                        .flat_map(|card| {
                            app.runtime
                                .cached_mobile_approval_snapshots(card.node_id)
                                .unwrap_or_default()
                        })
                        .collect();
                    LiveProject {
                        agents,
                        approvals,
                        questions: app.question_overview_rows(&key.1),
                    }
                });
                (key.clone(), project)
            })
            .collect()
    }
}
