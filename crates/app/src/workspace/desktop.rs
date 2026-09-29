use crate::app::MultisoulApp;
use gpui::Entity;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use storage::{DesktopAreaPreview, DesktopProjectArea, DesktopWorkspace, DesktopWorkspaceStore};
use ui::component::input::InputState;

mod agent;
mod agent_preview;
mod area;
mod controller;
mod dialogs;
mod dialogs_other;
mod dispatch;
mod management;
pub(crate) mod sessions;
mod view;
mod workspace_card;

#[derive(Clone)]
pub(crate) enum DesktopDeleteTarget {
    Workspace {
        id: String,
        name: String,
    },
    Area {
        workspace_id: String,
        area_id: String,
        name: String,
    },
}

pub(crate) struct DesktopAgentCreation {
    pub(crate) workspace_id: String,
    pub(crate) area_id: String,
    pub(crate) profile: canvas::AgentProfile,
    pub(crate) definition_id: Option<i64>,
    pub(crate) choices: Vec<PathBuf>,
    pub(crate) loading: bool,
}

#[derive(Clone)]
pub(crate) struct DesktopDispatchReceipt {
    pub(crate) project_name: String,
    pub(crate) agent_name: String,
    pub(crate) status: String,
    pub(crate) submitted: bool,
}

pub(crate) struct DesktopWorkspaceUiState {
    pub(crate) store: DesktopWorkspaceStore,
    pub(crate) workspaces: Vec<DesktopWorkspace>,
    pub(crate) previews: HashMap<String, Vec<DesktopAreaPreview>>,
    pub(crate) active_id: Option<String>,
    pub(crate) areas: Vec<DesktopProjectArea>,
    pub(crate) active_area_id: Option<String>,
    pub(crate) expanded_area_id: Option<String>,
    pub(crate) loading_area_ids: HashSet<(String, String)>,
    pub(crate) discarded_area_loads: HashSet<(String, String)>,
    pub(crate) area_errors: HashMap<String, String>,
    pub(crate) sessions: HashMap<(String, String), Entity<MultisoulApp>>,
    pub(crate) search_query: String,
    pub(crate) error: Option<String>,
    pub(crate) busy: bool,
    pub(crate) rename_target: Option<String>,
    pub(crate) rename_input: Option<Entity<InputState>>,
    pub(crate) switcher_open: bool,
    pub(crate) switcher_query: String,
    pub(crate) switcher_input: Option<Entity<InputState>>,
    pub(crate) expanded_by_workspace: HashMap<String, Option<String>>,
    pub(crate) view_save_task: Option<gpui::Task<()>>,
    pub(crate) pending_delete: Option<DesktopDeleteTarget>,
    pub(crate) pending_project_path: Option<PathBuf>,
    pub(crate) agent_creation: Option<DesktopAgentCreation>,
    pub(crate) dispatch_receipts: HashMap<String, Vec<DesktopDispatchReceipt>>,
    pub(crate) refresh_started: bool,
}

impl DesktopWorkspaceUiState {
    pub(crate) fn load(store: DesktopWorkspaceStore) -> Self {
        let (workspaces, error) = match store.list() {
            Ok(workspaces) => (workspaces, None),
            Err(error) => (
                Vec::new(),
                Some(format!("Could not load workspaces: {error}")),
            ),
        };
        Self {
            store,
            workspaces,
            previews: HashMap::new(),
            active_id: None,
            areas: Vec::new(),
            active_area_id: None,
            expanded_area_id: None,
            loading_area_ids: HashSet::new(),
            discarded_area_loads: HashSet::new(),
            area_errors: HashMap::new(),
            sessions: HashMap::new(),
            search_query: String::new(),
            error,
            busy: false,
            rename_target: None,
            rename_input: None,
            switcher_open: false,
            switcher_query: String::new(),
            switcher_input: None,
            expanded_by_workspace: HashMap::new(),
            view_save_task: None,
            pending_delete: None,
            pending_project_path: None,
            agent_creation: None,
            dispatch_receipts: HashMap::new(),
            refresh_started: false,
        }
    }

    pub(crate) fn filtered(&self) -> Vec<&DesktopWorkspace> {
        let query = self.search_query.trim().to_lowercase();
        self.workspaces
            .iter()
            .filter(|item| query.is_empty() || item.name.to_lowercase().contains(&query))
            .collect()
    }

    pub(crate) fn active(&self) -> Option<&DesktopWorkspace> {
        let active = self.active_id.as_deref()?;
        self.workspaces.iter().find(|item| item.id == active)
    }
}

impl MultisoulApp {
    pub(crate) fn detach_desktop_host_window(&mut self, cx: &mut gpui::Context<Self>) {
        self.detach_visible_desktop_area(cx);
        self.window_active = false;
        self.dialogue.on_window_closed();
        cx.notify();
    }

    pub(crate) fn attach_desktop_host_window(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.window_active = true;
        if let (Some(workspace_id), Some(area_id)) = (
            self.desktop_workspace.active_id.as_ref(),
            self.desktop_workspace.expanded_area_id.as_ref(),
        ) {
            if let Some(session) = self
                .desktop_workspace
                .sessions
                .get(&(workspace_id.clone(), area_id.clone()))
            {
                session.update(cx, |session, cx| {
                    session.attach_desktop_area_window(window, cx)
                });
            }
        }
        self.start_dialogue_turn_loop(window, cx);
        self.start_cef_message_pump_loop(window, cx);
        cx.notify();
    }

    pub(crate) fn stop_desktop_sessions(&mut self, cx: &mut gpui::Context<Self>) {
        let sessions = std::mem::take(&mut self.desktop_workspace.sessions);
        for session in sessions.into_values() {
            session.update(cx, |session, cx| session.stop_project(cx));
        }
        cx.notify();
    }
}

#[cfg(feature = "e2e-test-support")]
impl MultisoulApp {
    pub fn desktop_area_node_count_for_e2e(&self, cx: &gpui::App) -> Option<usize> {
        let workspace_id = self.desktop_workspace.active_id.as_ref()?;
        let area_id = self.desktop_workspace.expanded_area_id.as_ref()?;
        let session = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.clone(), area_id.clone()))?;
        Some(session.read(cx).runtime.document.nodes().len())
    }

    pub fn desktop_loaded_area_count_for_e2e(&self) -> usize {
        let Some(workspace_id) = self.desktop_workspace.active_id.as_ref() else {
            return 0;
        };
        self.desktop_workspace
            .sessions
            .keys()
            .filter(|(id, _)| id == workspace_id)
            .count()
    }

    pub fn desktop_area_live_count_for_e2e(
        &self,
        workspace_id: &str,
        area_id: &str,
        cx: &gpui::App,
    ) -> Option<usize> {
        let session = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.to_owned(), area_id.to_owned()))?;
        Some(session.read(cx).runtime.live_len())
    }

    pub fn desktop_area_running_count_for_e2e(
        &self,
        workspace_id: &str,
        area_id: &str,
        cx: &gpui::App,
    ) -> Option<usize> {
        let session = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.to_owned(), area_id.to_owned()))?;
        Some(session.read(cx).runtime.running_terminal_count())
    }

    pub fn desktop_dispatch_statuses_for_e2e(&self, workspace_id: &str) -> Vec<String> {
        self.desktop_workspace
            .dispatch_receipts
            .get(workspace_id)
            .into_iter()
            .flat_map(|receipts| receipts.iter().map(|receipt| receipt.status.clone()))
            .collect()
    }

    pub fn desktop_stop_area_runtime_for_e2e(
        &mut self,
        workspace_id: &str,
        area_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(session) = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.to_owned(), area_id.to_owned()))
        {
            session.update(cx, |session, _| session.runtime.kill_all());
        }
    }

    pub fn desktop_set_expanded_pty_factory_for_e2e(
        &mut self,
        factory: runtime::PtyFactory,
        cx: &mut gpui::Context<Self>,
    ) {
        if let (Some(workspace_id), Some(area_id)) = (
            self.desktop_workspace.active_id.as_ref(),
            self.desktop_workspace.expanded_area_id.as_ref(),
        ) {
            if let Some(session) = self
                .desktop_workspace
                .sessions
                .get(&(workspace_id.clone(), area_id.clone()))
            {
                session.update(cx, |session, _| session.runtime.set_pty_factory(factory));
            }
        }
    }

    pub fn desktop_expanded_agent_ids_for_e2e(&self, cx: &gpui::App) -> Vec<u64> {
        let (Some(workspace_id), Some(area_id)) = (
            self.desktop_workspace.active_id.as_ref(),
            self.desktop_workspace.expanded_area_id.as_ref(),
        ) else {
            return Vec::new();
        };
        let Some(session) = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.clone(), area_id.clone()))
        else {
            return Vec::new();
        };
        session
            .read(cx)
            .runtime
            .document
            .nodes()
            .iter()
            .filter_map(|node| {
                matches!(node.kind, canvas::CanvasNodeKind::Agent(_)).then_some(node.id)
            })
            .collect()
    }

    pub fn desktop_agent_grid_text_for_e2e(
        &self,
        workspace_id: &str,
        area_id: &str,
        node_id: u64,
        cx: &gpui::App,
    ) -> Option<String> {
        let session = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.to_owned(), area_id.to_owned()))?;
        let runtime::NodeView::Term { snapshot, .. } =
            session.read(cx).runtime.node_view(node_id)?;
        Some(
            snapshot
                .rows
                .iter()
                .map(|row| row.iter().map(|cell| cell.c).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }

    pub fn desktop_drain_area_outputs_for_e2e(&mut self, cx: &mut gpui::Context<Self>) {
        for session in self.desktop_workspace.sessions.values() {
            session.update(cx, |session, cx| {
                session.runtime.drain_all();
                cx.notify();
            });
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/desktop_workspace_state_tests.rs"]
mod tests;
