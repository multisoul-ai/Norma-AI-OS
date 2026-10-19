use crate::app::MultisoulApp;
use gpui::Entity;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use storage::{DesktopAreaPreview, DesktopProjectArea, DesktopWorkspace, DesktopWorkspaceStore};
use ui::component::button::ButtonCustomVariant;
use ui::component::input::InputState;

const GLASS_TEXT: u32 = 0xfffffff5;
const GLASS_TEXT_SECONDARY: u32 = 0xffffffb8;
const GLASS_TEXT_MUTED: u32 = 0xffffff85;

fn glass_button_variant(theme: ui::Theme, cx: &gpui::App) -> ButtonCustomVariant {
    let chrome = status_capsule::capsule_chrome(theme);
    ButtonCustomVariant::new(cx)
        .color(gpui::rgba(chrome.rest).into())
        .foreground(gpui::rgba(GLASS_TEXT).into())
        .border(gpui::rgba(theme.dock_line).into())
        .hover(gpui::rgba(chrome.hover).into())
        .active(gpui::rgba(chrome.active).into())
}

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
mod dispatch;
mod drawers;
mod home;
mod home_card;
mod launchpad;
mod layout;
mod management;
mod snapshot;
mod status;
mod status_capsule;
use management::DesktopRenameTarget;
mod motion_log;
mod navigation;
mod overview_rows;
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

#[derive(Clone)]
pub(crate) struct DesktopDispatchReceipt {
    pub(crate) project_name: String,
    pub(crate) agent_name: String,
    pub(crate) status: String,
    pub(crate) submitted: bool,
}

#[derive(Clone)]
pub(crate) struct DesktopArtifact {
    pub(crate) area_id: String,
    pub(crate) node_id: canvas::NodeId,
    pub(crate) agent_profile: String,
    pub(crate) turn_id: String,
    pub(crate) path: PathBuf,
    pub(crate) source_path: String,
    pub(crate) version: String,
    pub(crate) saved_at_unix: u64,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub(crate) struct DesktopArtifactVersion {
    pub(crate) node_id: canvas::NodeId,
    pub(crate) fingerprint: String,
    pub(crate) source_path: String,
}

pub(crate) struct DesktopWorkspaceUiState {
    pub(crate) store: DesktopWorkspaceStore,
    pub(crate) drawers: runtime::desktop_drawers::DesktopDrawerState,
    pub(crate) left_open_before_project: Option<bool>,
    pub(crate) drawer_motion: drawers::motion::DesktopAssistantDrawerMotion,
    left_view_slide: drawers::left_view_slide::LeftViewSlide,
    pub(crate) left_chat_tab_focus: Option<gpui::FocusHandle>,
    pub(crate) left_overview_tab_focus: Option<gpui::FocusHandle>,
    pub(crate) left_overview_focus: Option<gpui::FocusHandle>,
    pub(crate) overview_float_fade: drawers::float::OverviewFloatFade,
    pub(crate) overview_drag: Option<drawers::float::OverviewDrag>,
    pub(crate) overview_float_focus: Option<gpui::FocusHandle>,
    pub(crate) overview_move_focus: Option<gpui::FocusHandle>,
    pub(crate) run_status_focus: Option<gpui::FocusHandle>,
    pub(crate) drawer_layout_save: Option<gpui::Task<()>>,
    pub(crate) drawer_layout_generation: Arc<AtomicU64>,
    pub(crate) drawer_widths: drawers::resize::DesktopDrawerWidths,
    pub(crate) drawer_resize: Option<drawers::resize::DesktopDrawerResizeSession>,
    pub(crate) frosted_wallpaper: drawers::material::FrostedWallpaperState,
    pub(crate) assistant_orb_phase: f32,
    pub(crate) assistant_orb_energy: f32,
    pub(crate) content_tabs: Vec<storage::DesktopContentTab>,
    pub(crate) workbench: drawers::content::workbench::WorkbenchState,
    pub(crate) content_browser: drawers::content::browser_toolbar::ContentBrowserState,
    pub(crate) content_tab_task: Option<gpui::Task<()>>,
    pub(crate) content_tab_generation: u64,
    pub(crate) workspace_load_generation: u64,
    pub(crate) content_previews: HashMap<String, Result<String, String>>,
    pub(crate) content_snapshots: HashMap<String, tempfile::TempPath>,
    pub(crate) content_resources: HashMap<String, crate::preview::local_content::LocalPreview>,
    pub(crate) content_loading: HashSet<String>,
    pub(crate) content_artifacts: Vec<DesktopArtifact>,
    pub(crate) artifact_revision: u64,
    pub(crate) artifact_refreshing: bool,
    pub(crate) status_cache: HashMap<String, storage::DesktopAreaStatus>,
    pub(crate) status_live_seen: HashSet<String>,
    pub(crate) activity_gate: norma::mobile::ActivityRefreshGate,
    pub(crate) activity_generation: u64,
    pub(crate) activity_summary: Option<norma::mobile::WorkspaceActivitySummary>,
    pub(crate) activity_view: Option<norma::mobile::WorkspaceActivityView>,
    pub(crate) activity_capsule: Option<norma::mobile::StatusCapsule>,
    pub(crate) focus_status_overview_row: bool,
    activity_overview: Option<Vec<activity_cache::OverviewArea>>,
    pub(crate) activity_run_overview: Option<norma::mobile::RunOverview>,
    pub(crate) overview_cursor: Option<usize>,
    pub(crate) overview_prelight: Option<(String, u64)>,
    pub(crate) workspaces: Vec<DesktopWorkspace>,
    pub(crate) previews: HashMap<String, Vec<DesktopAreaPreview>>,
    pub(crate) active_id: Option<String>,
    pub(crate) areas: Vec<DesktopProjectArea>,
    pub(crate) active_area_id: Option<String>,
    pub(crate) selected_area_id: Option<String>,
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
    pub(crate) dispatch_receipts: HashMap<String, Vec<DesktopDispatchReceipt>>,
    pub(crate) refresh_started: bool,
}

impl DesktopWorkspaceUiState {
    pub(crate) fn left_view_slide_active(&self) -> bool {
        self.left_view_slide.is_active()
    }

    pub(crate) fn tick_left_view_slide(
        &mut self,
        delta: std::time::Duration,
        reduce_motion: bool,
    ) -> bool {
        self.left_view_slide.tick(delta, reduce_motion)
    }

    pub(crate) fn overview_float_fade_active(&self) -> bool {
        self.overview_float_fade.is_active()
    }

    pub(crate) fn tick_overview_float_fade(
        &mut self,
        delta: std::time::Duration,
        reduce_motion: bool,
    ) -> bool {
        self.overview_float_fade.tick(delta, reduce_motion)
    }

    pub(crate) fn advance_assistant_orb(
        &mut self,
        delta: std::time::Duration,
        reduce_motion: bool,
        mode: drawers::orb::OrbMode,
    ) -> bool {
        let next = drawers::orb::advance_orb_phase(self.assistant_orb_phase, delta, reduce_motion);
        let energy =
            drawers::orb::advance_orb_energy(self.assistant_orb_energy, mode, delta, reduce_motion);
        let changed = next != self.assistant_orb_phase || energy != self.assistant_orb_energy;
        self.assistant_orb_phase = next;
        self.assistant_orb_energy = energy;
        changed
    }

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
            left_open_before_project: None,
            drawer_motion: drawers::motion::DesktopAssistantDrawerMotion::default(),
            left_view_slide: drawers::left_view_slide::LeftViewSlide::default(),
            left_chat_tab_focus: None,
            left_overview_tab_focus: None,
            left_overview_focus: None,
            overview_float_fade: drawers::float::OverviewFloatFade::default(),
            overview_drag: None,
            overview_float_focus: None,
            overview_move_focus: None,
            run_status_focus: None,
            drawer_layout_save: None,
            drawer_layout_generation: Arc::new(AtomicU64::new(0)),
            drawer_widths: drawers::resize::DesktopDrawerWidths::default(),
            drawer_resize: None,
            frosted_wallpaper: drawers::material::FrostedWallpaperState::default(),
            assistant_orb_phase: 0.0,
            assistant_orb_energy: 0.1,
            content_tabs: Vec::new(),
            workbench: Default::default(),
            content_browser: Default::default(),
            content_tab_task: None,
            content_tab_generation: 0,
            workspace_load_generation: 0,
            content_previews: HashMap::new(),
            content_snapshots: HashMap::new(),
            content_resources: HashMap::new(),
            content_loading: HashSet::new(),
            content_artifacts: Vec::new(),
            artifact_revision: 0,
            artifact_refreshing: false,
            status_cache: HashMap::new(),
            status_live_seen: HashSet::new(),
            activity_gate: norma::mobile::ActivityRefreshGate::default(),
            activity_generation: 0,
            activity_summary: None,
            activity_view: None,
            activity_capsule: None,
            focus_status_overview_row: false,
            activity_overview: None,
            activity_run_overview: None,
            overview_cursor: None,
            overview_prelight: None,
            workspaces,
            previews: HashMap::new(),
            active_id: None,
            areas: Vec::new(),
            active_area_id: None,
            selected_area_id: None,
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
    pub(super) fn desktop_theme(&self) -> ui::Theme {
        if self.theme_id.is_dark() {
            self.theme
        } else {
            ui::Theme::sapphire_night()
        }
    }

    pub(super) fn desktop_input_fill(&self) -> u32 {
        self.desktop_theme().panel_high
    }

    pub(super) fn sync_desktop_input_theme(&self, expanded: bool, cx: &mut gpui::Context<Self>) {
        let light_dialog_open = self.settings_panel.open
            || self.clone_project.is_open()
            || self.desktop_workspace.add_project_open
            || self.desktop_workspace.pending_project_path.is_some()
            || self.desktop_workspace.pending_delete.is_some()
            || self.desktop_workspace.rename_target.is_some()
            || self.desktop_workspace.create_input.is_some();
        let id = if expanded || light_dialog_open || self.theme_id.is_dark() {
            self.theme_id
        } else {
            ui::ThemeId::SapphireNight
        };
        let foreground: gpui::Hsla = gpui::rgba(ui::Theme::for_id(id).text_primary).into();
        if ui::component::Theme::global(cx).colors.foreground != foreground {
            ui::sync_component_theme(id, cx);
        }
    }

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
        self.flush_left_drawer_view_on_exit();
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
