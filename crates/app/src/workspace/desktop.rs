use crate::app::MultisoulApp;
use gpui::Entity;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use storage::{DesktopAreaPreview, DesktopProjectArea, DesktopWorkspace, DesktopWorkspaceStore};
use ui::component::button::ButtonCustomVariant;
use ui::component::input::InputState;

fn glass_button_variant(theme: ui::Theme, cx: &gpui::App) -> ButtonCustomVariant {
    ButtonCustomVariant::new(cx)
        .color(gpui::rgba(theme.panel).into())
        .foreground(gpui::rgba(theme.text_primary).into())
        .border(gpui::rgba(theme.dock_line).into())
        .hover(gpui::rgba(theme.panel_high).into())
        .active(gpui::rgba(theme.node_fill).into())
}

#[cfg(test)]
#[path = "../../tests/unit/desktop_surface_theme_tests.rs"]
mod surface_theme_tests;

mod activity_cache;
mod agent;
mod agent_preview;
mod area;
mod artifacts;
mod camera;
mod camera_frames;
mod context;
mod controller;
mod creation;
mod dialogs;
mod dialogs_other;
mod directory_navigation;
mod dispatch;
#[cfg(test)]
#[path = "../../tests/support/desktop_embedded_project.rs"]
mod test_embedded_project;
pub(crate) use dispatch::{AgentLocation, ArtifactLocation};
mod drawers;
pub(crate) use drawers::overview_questions::QuestionEditor;
mod home;
mod home_card;
mod launchpad;
mod layout;
mod management;
mod snapshot;
mod status;
pub(crate) mod surface_theme;
use management::DesktopRenameTarget;
mod motion_log;
mod navigation;
mod preview_canvas;
mod project_area;
mod project_layout;
mod project_picker;
pub(crate) use camera::desktop_scene_rect;
pub(crate) mod mobile;
mod project_switch;
mod project_world;
mod scene;
pub(crate) mod sessions;
mod shell;
mod view;
mod workspace_card;

#[cfg(test)]
#[path = "../../tests/unit/desktop_motion_log_tests.rs"]
mod motion_log_tests;

#[cfg(test)]
#[path = "../../tests/unit/desktop_project_picker_tests.rs"]
mod project_picker_tests;

use camera::DesktopCanvasTransition;

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

mod content_state;
mod island;
mod island_actions;
pub(crate) use content_state::artifact_read_key;
pub(crate) use content_state::{DesktopArtifact, DesktopArtifactVersion};

pub(crate) struct DesktopWorkspaceUiState {
    pub(crate) store: DesktopWorkspaceStore,
    pub(crate) drawers: runtime::desktop_drawers::DesktopDrawerState,
    pub(crate) drawer_motion: drawers::motion::DesktopAssistantDrawerMotion,
    pub(crate) drawer_widths: drawers::resize::DesktopDrawerWidths,
    pub(crate) drawer_resize: Option<drawers::resize::DesktopDrawerResizeSession>,
    pub(crate) frosted_wallpaper: drawers::material::FrostedWallpaperState,
    pub(crate) content_tabs: Vec<storage::DesktopContentTab>,
    pub(crate) workbench: drawers::content::workbench::WorkbenchState,
    pub(crate) content_browser: drawers::content::browser_toolbar::ContentBrowserState,
    pub(crate) content_tab_task: Option<gpui::Task<()>>,
    pub(crate) content_tab_generation: u64,
    pub(crate) content_workspace_epoch: u64,
    pub(crate) workspace_load_generation: u64,
    pub(crate) content_previews: HashMap<String, Result<String, ui::i18n::LocalizedText>>,
    pub(crate) content_snapshots: HashMap<String, tempfile::TempPath>,
    pub(crate) content_resources: HashMap<String, crate::preview::local_content::LocalPreview>,
    pub(crate) content_loading: HashSet<String>,
    pub(crate) content_load_sequence: u64,
    pub(crate) content_load_versions: HashMap<String, u64>,
    pub(crate) content_artifacts: Vec<DesktopArtifact>,
    pub(crate) artifact_revision: u64,
    pub(crate) artifact_refreshing: bool,
    pub(crate) status_cache: HashMap<String, storage::DesktopAreaStatus>,
    pub(crate) status_live_seen: HashSet<String>,
    activity_monitor: activity_cache::ActivityMonitor,
    pub(crate) activity_gate: norma::mobile::ActivityRefreshGate,
    pub(crate) activity_generation: u64,
    pub(crate) activity_summary: Option<norma::mobile::WorkspaceActivitySummary>,
    pub(crate) activity_view: Option<norma::mobile::WorkspaceActivityView>,
    pub(crate) activity_capsule: Option<norma::mobile::StatusCapsule>,
    activity_overview: Option<Vec<activity_cache::OverviewArea>>,
    pub(crate) activity_run_overview: Option<norma::mobile::RunOverview>,
    pub(crate) activity_updated_at_ms: Option<i64>,
    pub(crate) workspaces: Vec<DesktopWorkspace>,
    pub(crate) previews: HashMap<String, Vec<DesktopAreaPreview>>,
    pub(crate) active_id: Option<String>,
    pub(crate) areas: Vec<DesktopProjectArea>,
    pub(crate) active_area_id: Option<String>,
    pub(crate) selected_area_id: Option<String>,
    pub(crate) directory: directory_navigation::DirectorySelection,
    pub(crate) expanded_area_id: Option<String>,
    pub(crate) workspace_rects: Arc<Mutex<HashMap<String, canvas::CanvasRect>>>,
    workspace_motion_rects: HashMap<String, canvas::CanvasRect>,
    pub(crate) area_rects: Arc<Mutex<HashMap<(String, String), canvas::CanvasRect>>>,
    project_layout: project_layout::ProjectLayout,
    transition: Option<DesktopCanvasTransition>,
    motion_log: Option<motion_log::DesktopMotionLog>,
    pending_navigation: Option<scene::DesktopNavigation>,
    transition_frame_pending: bool,
    transition_last_tick: Instant,
    #[cfg(feature = "e2e-test-support")]
    animate_camera_for_e2e: bool,
    pub(crate) loading_area_ids: HashSet<(String, String)>,
    pub(crate) discarded_area_loads: HashSet<(String, String)>,
    pub(crate) area_errors: HashMap<String, ui::i18n::LocalizedText>,
    pub(crate) sessions: HashMap<(String, String), Entity<MultisoulApp>>,
    pub(crate) search_query: String,
    pub(crate) home: home::HomeState,
    pub(crate) error: Option<ui::i18n::LocalizedText>,
    pub(crate) busy: bool,
    pub(crate) rename_target: Option<DesktopRenameTarget>,
    pub(crate) rename_input: Option<Entity<InputState>>,
    pub(crate) create_input: Option<Entity<InputState>>,
    pub(crate) actions_workspace_id: Option<String>,
    pub(crate) actions_area_id: Option<String>,
    pub(crate) switcher_open: bool,
    // Retain the directory presentation while its controls close during camera motion.
    pub(crate) home_directory: bool,
    switcher_return: Option<(String, Option<String>)>,
    pub(crate) switcher_query: String,
    pub(crate) switcher_input: Option<Entity<InputState>>,
    pub(crate) expanded_by_workspace: HashMap<String, Option<String>>,
    pub(crate) scroll_save_task: Option<gpui::Task<()>>,
    pub(crate) view_save_task: Option<gpui::Task<()>>,
    pub(crate) pending_delete: Option<DesktopDeleteTarget>,
    pub(crate) pending_project_path: Option<PathBuf>,
    pub(crate) pending_project_source: workspace::WorkspaceSource,
    pub(crate) add_project_open: bool,
    pub(crate) refresh_started: bool,
}

impl DesktopWorkspaceUiState {
    pub(crate) fn empty(store: DesktopWorkspaceStore) -> Self {
        Self::from_catalog(store, Vec::new(), None)
    }

    #[cfg(test)]
    pub(crate) fn load(store: DesktopWorkspaceStore) -> Self {
        let (workspaces, error) = match store.list() {
            Ok(workspaces) => (workspaces, None),
            Err(error) => (
                Vec::new(),
                Some(ui::i18n::message(
                    "workspace.could_not_load_workspaces_error",
                    &[("error", (error).to_string())],
                )),
            ),
        };
        Self::from_catalog(store, workspaces, error)
    }

    fn from_catalog(
        store: DesktopWorkspaceStore,
        workspaces: Vec<DesktopWorkspace>,
        error: Option<ui::i18n::LocalizedText>,
    ) -> Self {
        Self {
            store,
            drawers: runtime::desktop_drawers::DesktopDrawerState::default(),
            drawer_motion: drawers::motion::DesktopAssistantDrawerMotion::default(),
            drawer_widths: drawers::resize::DesktopDrawerWidths::default(),
            drawer_resize: None,
            frosted_wallpaper: drawers::material::FrostedWallpaperState::default(),
            content_tabs: Vec::new(),
            // 「所有文件」默认折叠，只在用户展开后才显示整棵树。
            workbench: drawers::content::workbench::WorkbenchState {
                files_section_closed: true,
                ..Default::default()
            },
            content_browser: Default::default(),
            content_tab_task: None,
            content_tab_generation: 0,
            content_workspace_epoch: 0,
            workspace_load_generation: 0,
            content_previews: HashMap::new(),
            content_snapshots: HashMap::new(),
            content_resources: HashMap::new(),
            content_loading: HashSet::new(),
            content_load_sequence: 0,
            content_load_versions: HashMap::new(),
            content_artifacts: Vec::new(),
            artifact_revision: 0,
            artifact_refreshing: false,
            status_cache: HashMap::new(),
            status_live_seen: HashSet::new(),
            activity_monitor: activity_cache::ActivityMonitor::default(),
            activity_gate: norma::mobile::ActivityRefreshGate::default(),
            activity_generation: 0,
            activity_summary: None,
            activity_view: None,
            activity_capsule: None,
            activity_overview: None,
            activity_run_overview: None,
            activity_updated_at_ms: None,
            workspaces,
            previews: HashMap::new(),
            active_id: None,
            areas: Vec::new(),
            active_area_id: None,
            selected_area_id: None,
            directory: Default::default(),
            expanded_area_id: None,
            workspace_rects: Arc::new(Mutex::new(HashMap::new())),
            workspace_motion_rects: HashMap::new(),
            area_rects: Arc::new(Mutex::new(HashMap::new())),
            project_layout: project_layout::ProjectLayout::default(),
            transition: None,
            motion_log: None,
            pending_navigation: None,
            transition_frame_pending: false,
            transition_last_tick: Instant::now(),
            #[cfg(feature = "e2e-test-support")]
            animate_camera_for_e2e: true,
            loading_area_ids: HashSet::new(),
            discarded_area_loads: HashSet::new(),
            area_errors: HashMap::new(),
            sessions: HashMap::new(),
            search_query: String::new(),
            home: Default::default(),
            error,
            busy: false,
            rename_target: None,
            rename_input: None,
            create_input: None,
            actions_workspace_id: None,
            actions_area_id: None,
            switcher_open: false,
            home_directory: true,
            switcher_return: None,
            switcher_query: String::new(),
            switcher_input: None,
            expanded_by_workspace: HashMap::new(),
            scroll_save_task: None,
            view_save_task: None,
            pending_delete: None,
            pending_project_path: None,
            pending_project_source: workspace::WorkspaceSource::Open,
            add_project_open: false,
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
        self.stop_gpt_live_voice(cx);
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
        self.stop_island_activity(cx);
        let sessions = std::mem::take(&mut self.desktop_workspace.sessions);
        for session in sessions.into_values() {
            session.update(cx, |session, cx| session.stop_project(cx));
        }
        cx.notify();
    }
}

#[cfg(feature = "e2e-test-support")]
mod e2e;

#[cfg(test)]
#[path = "../../tests/unit/desktop_workspace_state_tests.rs"]
mod tests;

pub(crate) use drawers::content::workbench::PendingWorkbenchAction;
