//! GPUI 主视图：持有 Runtime，装配画布/终端渲染，处理快捷键与帧级 drain。

use crate::assets::DEFAULT_WALLPAPER_ASSET;
use crate::canvas_view::{render_canvas, InteractionState};
use crate::cards::term::view::keystroke_to_input;
use crate::master_layout::{
    apply_workspace_reflow_for_area, close_node_and_update_layout_for_area,
    enter_master_layout_for_area, layout_gap_for_zoom, master_target_from_hover_or_selection,
    reapply_master_layout_for_area, restore_master_layout_for_area, term_grid_size_for_card,
    MasterLayoutState,
};
#[cfg(test)]
use crate::master_layout::{close_node_and_reflow_for_area, workspace_reflow_config_for_zoom};
use crate::motion::MotionStore;
use crate::persistence::PersistenceScheduler;
use crate::workspace_state::WorkspaceState;
use canvas::{AgentProfile, CanvasRect, NodeId, NodeType, Vec2};
use gpui::{
    actions, div, prelude::*, px, ClickEvent, Context, FocusHandle, Focusable, ImageSource,
    KeyDownEvent, Window,
};
use runtime::{route, InputEvent, InputMode, NodeInput, Route, Runtime};
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;
use ui::Theme;

#[path = "app_chrome.rs"]
mod workspace_chrome;

actions!(
    multisoul,
    [
        NewTerminal,
        NewClaudeAgent,
        NewCodexAgent,
        OpenProject,
        ExitFocus,
        ReflowLayout,
        ToggleMasterLayout
    ]
);

const WALLPAPER_IMAGE_OPACITY: f32 = 1.0;
const WALLPAPER_SHADE_TOP_ALPHA: u8 = 0x00;
const WALLPAPER_SHADE_BOTTOM_ALPHA: u8 = 0x00;

pub struct MultisoulApp {
    pub runtime: Runtime,
    pub theme: Theme,
    pub interaction: InteractionState,
    pub(crate) motion: MotionStore,
    pub(crate) master_layout: MasterLayoutState,
    pub(crate) persistence: PersistenceScheduler,
    pub(crate) screen: AppScreen,
    pub(crate) workspace_state: WorkspaceState,
    pub focus_handle: FocusHandle,
    wallpaper: WorkspaceWallpaper,
    pub(crate) last_reflow_window_size: Option<(f32, f32)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AppScreen {
    Launchpad,
    Canvas,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum WorkspaceWallpaper {
    Embedded(&'static str),
    File(PathBuf),
}

impl WorkspaceWallpaper {
    fn image_source(&self) -> ImageSource {
        match self {
            Self::Embedded(asset_path) => (*asset_path).into(),
            Self::File(path) => path.clone().into(),
        }
    }
}

impl MultisoulApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // 后台帧循环：每 ~12ms drain 一次（8–16ms 批处理窗口），dirty 才 repaint。
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(12))
                    .await;
                let go = this.update(cx, |app, cx| {
                    let runtime_changed = app.runtime.drain_all();
                    let motion_changed = app.motion.tick(Duration::from_millis(12));
                    let persistence_changed = app.tick_persistence(Duration::from_millis(12));
                    if runtime_changed || motion_changed || persistence_changed {
                        cx.notify();
                    }
                });
                if go.is_err() {
                    break; // 视图已释放
                }
            }
        })
        .detach();

        let wallpaper = wallpaper_from_env();

        MultisoulApp {
            runtime: Runtime::new(),
            theme: Theme::dark(),
            interaction: InteractionState::default(),
            motion: MotionStore::from_env(),
            master_layout: MasterLayoutState::default(),
            persistence: PersistenceScheduler::default(),
            screen: AppScreen::Launchpad,
            workspace_state: WorkspaceState::load_default(),
            focus_handle: cx.focus_handle(),
            wallpaper,
            last_reflow_window_size: None,
        }
    }

    /// Cmd+N：在当前可见画布区域内新建 Terminal 节点，并触发三列平铺 reflow。
    fn on_new_terminal(&mut self, _: &NewTerminal, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.screen, AppScreen::Canvas) {
            self.workspace_state.error = Some("Open a project before creating terminals.".into());
            cx.notify();
            return;
        }

        let bounds = self
            .visible_canvas_rect(window)
            .inset(self.layout_gap_canvas());
        let (cols, rows) =
            term_grid_size_for_card(NodeType::Term, bounds, self.runtime.document.viewport.zoom);
        match self.runtime.spawn_terminal(bounds, "/bin/zsh", cols, rows) {
            Ok(node_id) => {
                tracing::info!(?node_id, "spawned terminal node");
                crate::spawn_focus::activate_spawned_node(&mut self.runtime, node_id);
                self.motion.spawn_node(node_id, bounds);
                self.apply_layout_for_current_mode(window);
                self.mark_canvas_dirty();
                self.mark_session_dirty();
            }
            Err(e) => {
                // SPEC §8：spawn 失败不 panic，仅 error 日志（节点内错误占位由 Task 15 渲染）。
                tracing::error!(error = %e, "failed to spawn terminal");
            }
        }
        cx.notify();
    }

    fn on_new_claude_agent(
        &mut self,
        _: &NewClaudeAgent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.screen, AppScreen::Canvas) {
            self.workspace_state.error = Some("Open a project before creating agents.".into());
            cx.notify();
            return;
        }
        self.spawn_agent_card(AgentProfile::Claude, window, cx);
    }

    fn on_new_codex_agent(
        &mut self,
        _: &NewCodexAgent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.screen, AppScreen::Canvas) {
            self.workspace_state.error = Some("Open a project before creating agents.".into());
            cx.notify();
            return;
        }
        self.spawn_agent_card(AgentProfile::Codex, window, cx);
    }

    fn spawn_agent_card(
        &mut self,
        profile: AgentProfile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = self
            .visible_canvas_rect(window)
            .inset(self.layout_gap_canvas());
        let (cols, rows) =
            term_grid_size_for_card(NodeType::Agent, bounds, self.runtime.document.viewport.zoom);
        match self.runtime.spawn_agent(profile, bounds, cols, rows) {
            Ok(node_id) => {
                tracing::info!(?node_id, ?profile, "spawned agent node");
                crate::spawn_focus::activate_spawned_node(&mut self.runtime, node_id);
                self.motion.spawn_node(node_id, bounds);
                self.apply_layout_for_current_mode(window);
                self.mark_canvas_dirty();
                self.mark_session_dirty();
            }
            Err(e) => {
                tracing::error!(error = %e, ?profile, "failed to spawn agent");
            }
        }
        cx.notify();
    }

    fn on_exit_focus(&mut self, _: &ExitFocus, _window: &mut Window, cx: &mut Context<Self>) {
        self.runtime.focus.exit_to_canvas();
        cx.notify();
    }

    fn on_reflow_layout(&mut self, _: &ReflowLayout, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.screen, AppScreen::Canvas) {
            return;
        }
        self.reflow_tiled_nodes(window);
        self.mark_canvas_dirty();
        cx.notify();
    }

    fn on_toggle_master_layout(
        &mut self,
        _: &ToggleMasterLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.screen, AppScreen::Canvas) {
            return;
        }

        let target =
            master_target_from_hover_or_selection(self.interaction.hovered_node, &self.runtime);
        let event = InputEvent::ToggleMasterLayout {
            active: self.master_layout.is_active(),
            target,
        };
        match route(&self.runtime.focus.mode(), &event) {
            Route::EnterMasterLayout(master_id) => {
                let area = self.visible_canvas_rect(window);
                let zoom = self.runtime.document.viewport.zoom;
                enter_master_layout_for_area(
                    &mut self.master_layout,
                    &mut self.runtime,
                    &mut self.motion,
                    master_id,
                    area,
                    zoom,
                );
            }
            Route::ExitMasterLayout => {
                let zoom = self.runtime.document.viewport.zoom;
                restore_master_layout_for_area(
                    &mut self.master_layout,
                    &mut self.runtime,
                    &mut self.motion,
                    zoom,
                );
            }
            _ => {}
        }
        self.mark_canvas_dirty();
        cx.notify();
    }

    fn on_open_project(&mut self, _: &OpenProject, _window: &mut Window, cx: &mut Context<Self>) {
        self.prompt_open_project(cx);
    }

    pub(crate) fn remove_node_and_reflow(&mut self, node_id: NodeId, window: &mut Window) -> bool {
        let area = self.visible_canvas_rect(window);
        let zoom = self.runtime.document.viewport.zoom;
        if self.interaction.hovered_node == Some(node_id) {
            self.interaction.hovered_node = None;
        }
        let changed = close_node_and_update_layout_for_area(
            &mut self.runtime,
            &mut self.motion,
            &mut self.master_layout,
            node_id,
            area,
            zoom,
        );
        if changed {
            self.mark_canvas_dirty();
            self.mark_session_dirty();
        }
        changed
    }

    fn visible_canvas_rect(&self, window: &mut Window) -> CanvasRect {
        let vp = self.runtime.document.viewport;
        let win = window.viewport_size();
        let top_left = vp.screen_to_canvas(Vec2::ZERO);
        let bottom_right =
            vp.screen_to_canvas(Vec2::new(f32::from(win.width), f32::from(win.height)));
        CanvasRect::new(
            top_left.x,
            top_left.y,
            bottom_right.x - top_left.x,
            bottom_right.y - top_left.y,
        )
    }

    fn layout_gap_canvas(&self) -> f32 {
        layout_gap_for_zoom(self.runtime.document.viewport.zoom)
    }

    fn apply_tiled_nodes(&mut self, window: &mut Window) {
        let area = self.visible_canvas_rect(window);
        let zoom = self.runtime.document.viewport.zoom;
        apply_workspace_reflow_for_area(&mut self.runtime, &mut self.motion, area, zoom);
    }

    pub(crate) fn reflow_tiled_nodes(&mut self, window: &mut Window) {
        self.master_layout.clear();
        self.apply_tiled_nodes(window);
    }

    fn apply_layout_for_current_mode(&mut self, window: &mut Window) {
        let area = self.visible_canvas_rect(window);
        let zoom = self.runtime.document.viewport.zoom;
        if self.master_layout.is_active()
            && reapply_master_layout_for_area(
                &mut self.master_layout,
                &mut self.runtime,
                &mut self.motion,
                area,
                zoom,
            )
        {
            return;
        }
        self.apply_tiled_nodes(window);
    }

    fn reflow_if_window_size_changed(&mut self, window: &mut Window) {
        let size = window.viewport_size();
        let current = (f32::from(size.width), f32::from(size.height));
        if self.last_reflow_window_size != Some(current) {
            self.last_reflow_window_size = Some(current);
            self.apply_layout_for_current_mode(window);
        }
    }

    /// 键盘事件：维护 space_held；聚焦终端时把按键编码写入对应 PTY。
    fn on_key_down(&mut self, ev: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.screen, AppScreen::Canvas) {
            return;
        }

        let is_space = ev.keystroke.key == "space";
        if is_space && matches!(self.runtime.focus.mode(), InputMode::CanvasSelect) {
            self.interaction.space_held = true;
        }

        if let InputMode::NodeFocused(node_id) = self.runtime.focus.mode() {
            if route(&self.runtime.focus.mode(), &InputEvent::Key)
                == Route::SendInputToNode(node_id)
            {
                if ev.keystroke.modifiers.platform && ev.keystroke.key == "v" {
                    if let Some(item) = cx.read_from_clipboard() {
                        if let Some(text) = item.text() {
                            if let Err(e) =
                                self.runtime.send_node_input(node_id, NodeInput::Text(text))
                            {
                                tracing::error!(error = %e, "paste send_node_input failed");
                            }
                            cx.notify();
                        }
                    }
                    return;
                }

                if crate::app_ime::should_defer_keystroke_to_ime(&ev.keystroke) {
                    return;
                }
                if let Some(input) = keystroke_to_input(&ev.keystroke) {
                    if let Err(e) = self.runtime.send_node_input(node_id, NodeInput::Key(input)) {
                        tracing::error!(error = %e, "send_node_input failed");
                    }
                }
                cx.notify();
            }
        }
    }

    fn on_key_up(&mut self, ev: &gpui::KeyUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.screen, AppScreen::Canvas) {
            return;
        }

        if ev.keystroke.key == "space" {
            self.interaction.space_held = false;
            cx.notify();
        }
    }

    fn on_toolbar_new_terminal(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.on_new_terminal(&NewTerminal, window, cx);
    }

    fn on_toolbar_new_claude_agent(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.spawn_agent_card(AgentProfile::Claude, window, cx);
    }

    fn on_toolbar_new_codex_agent(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.spawn_agent_card(AgentProfile::Codex, window, cx);
    }
}

impl Focusable for MultisoulApp {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MultisoulApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div()
            .key_context("MultisoulApp")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_new_terminal))
            .on_action(cx.listener(Self::on_new_claude_agent))
            .on_action(cx.listener(Self::on_new_codex_agent))
            .on_action(cx.listener(Self::on_open_project))
            .on_action(cx.listener(Self::on_exit_focus))
            .on_action(cx.listener(Self::on_reflow_layout))
            .on_action(cx.listener(Self::on_toggle_master_layout))
            .on_key_down(cx.listener(Self::on_key_down))
            .on_key_up(cx.listener(Self::on_key_up))
            .size_full()
            .relative()
            .bg(gpui::rgba(self.theme.graphite))
            .text_size(px(13.0))
            .child(crate::app_ime::TerminalImeElement::new(
                self.focus_handle.clone(),
                cx.weak_entity(),
            ));

        match self.screen {
            AppScreen::Launchpad => {
                root = root.child(self.render_launchpad(window, cx));
            }
            AppScreen::Canvas => {
                self.reflow_if_window_size_changed(window);
                root = root
                    .child(self.render_workspace_backdrop())
                    .child(render_canvas(self, window, cx))
                    .child(self.render_workspace_chrome(window, cx));
            }
        }

        root
    }
}

fn wallpaper_from_env() -> WorkspaceWallpaper {
    wallpaper_from_env_value(std::env::var_os("MULTISOUL_WALLPAPER"))
}

fn wallpaper_from_env_value(env_value: Option<OsString>) -> WorkspaceWallpaper {
    let Some(path) = env_value.map(PathBuf::from) else {
        return WorkspaceWallpaper::Embedded(DEFAULT_WALLPAPER_ASSET);
    };

    if path.is_file() {
        WorkspaceWallpaper::File(path)
    } else {
        tracing::warn!(
            path = %path.display(),
            default_asset = DEFAULT_WALLPAPER_ASSET,
            "MULTISOUL_WALLPAPER does not point to a readable file; using embedded default wallpaper"
        );
        WorkspaceWallpaper::Embedded(DEFAULT_WALLPAPER_ASSET)
    }
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "master_layout_tests.rs"]
mod master_layout_tests;
