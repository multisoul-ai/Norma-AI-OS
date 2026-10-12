use crate::app::MultisoulApp;
#[cfg(feature = "e2e-test-support")]
use gpui::AppContext as _;
use gpui::Entity;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use storage::{DesktopAreaPreview, DesktopProjectArea, DesktopWorkspace, DesktopWorkspaceStore};
use ui::component::button::ButtonCustomVariant;
use ui::component::input::InputState;
use ui::icons::with_alpha;

const GLASS_TEXT: u32 = 0xfffffff5;
const GLASS_TEXT_SECONDARY: u32 = 0xffffffb8;
const GLASS_TEXT_MUTED: u32 = 0xffffff85;

fn glass_button_variant(theme: ui::Theme, cx: &gpui::App) -> ButtonCustomVariant {
    ButtonCustomVariant::new(cx)
        .color(gpui::rgba(with_alpha(theme.dock, 0xa0)).into())
        .foreground(gpui::rgba(GLASS_TEXT).into())
        .border(gpui::rgba(theme.dock_line).into())
        .hover(gpui::rgba(with_alpha(theme.accent_blue, 0x78)).into())
        .active(gpui::rgba(with_alpha(theme.accent_blue, 0x9c)).into())
}

mod agent;
mod agent_preview;
mod area;
mod camera;
mod camera_frames;
mod controller;
mod creation;
mod dialogs;
mod dialogs_other;
#[cfg(feature = "e2e-test-support")]
mod dispatch;
mod home;
mod home_card;
mod launchpad;
mod layout;
mod management;
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
    pub fn desktop_wallpaper_render_key_for_e2e(&self) -> String {
        self.wallpaper_settings.render_id_key()
    }

    pub fn desktop_set_catalog_path_for_e2e(&mut self, path: PathBuf) {
        self.desktop_workspace.store = DesktopWorkspaceStore::new(path);
    }

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

    /// Exercises the existing workspace dispatcher without showing a second message field.
    pub fn desktop_send_command_for_e2e(
        &mut self,
        text: &str,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |input, cx| input.set_value(text.to_owned(), window, cx));
        self.send_desktop_workspace_command(input, window, cx);
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

    pub fn desktop_set_area_pty_factory_for_e2e(
        &mut self,
        area_id: &str,
        factory: runtime::PtyFactory,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(workspace_id) = self.desktop_workspace.active_id.as_ref() else {
            return;
        };
        if let Some(session) = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.clone(), area_id.to_owned()))
        {
            session.update(cx, |session, _| session.runtime.set_pty_factory(factory));
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
