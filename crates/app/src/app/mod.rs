//! GPUI 主视图：持有 Runtime，装配画布/终端渲染，处理快捷键与帧级 drain。

#[cfg(test)]
use crate::assets::DEFAULT_WALLPAPER_ASSET;
use crate::canvas::interaction::InteractionState;
use crate::canvas::master_layout::{
    apply_workspace_reflow_for_area, close_node_and_update_layout_for_area, layout_gap_for_zoom,
    reapply_master_layout_for_area, term_grid_size_for_card, MasterLayoutState,
};
#[cfg(test)]
use crate::canvas::master_layout::{
    close_node_and_reflow_for_area, workspace_reflow_config_for_zoom,
};
use crate::canvas::spawn_focus;
use crate::canvas::view::render_canvas;
use crate::motion::MotionStore;
use crate::norma::server::NormaServer;
use crate::preview::PreviewStore;
use crate::settings::agent_hooks::AgentHookServer;
use crate::settings::panel::SettingsPanelState;
use crate::workspace::persistence::PersistenceScheduler;
use crate::workspace::state::WorkspaceState;
use canvas::{AgentProfile, CanvasRect, NodeId, NodeType, Vec2};
use gpui::{actions, div, prelude::*, px, ClickEvent, Context, FocusHandle, Focusable, Window};
use runtime::Runtime;
use std::time::Duration;
use ui::Theme;

mod chrome;
mod ime;
mod ime_geometry;
mod keyboard;
mod lifecycle;
mod voice;
mod wallpaper;

#[cfg(test)]
use self::wallpaper::wallpaper_from_env_value;
use self::wallpaper::{wallpaper_from_env, WorkspaceWallpaper};

actions!(
    multisoul,
    [
        NewTerminal,
        NewClaudeAgent,
        NewCodexAgent,
        CopyCardContent,
        OpenPreviewFile,
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
    pub(crate) previews: PreviewStore,
    pub(crate) master_layout: MasterLayoutState,
    pub(crate) persistence: PersistenceScheduler,
    pub(crate) settings_panel: SettingsPanelState,
    pub(crate) norma_server: NormaServer,
    pub(crate) agent_hook_server: AgentHookServer,
    pub(crate) screen: AppScreen,
    pub(crate) workspace_state: WorkspaceState,
    pub(crate) voice: crate::app::voice::control::VoiceControlState,
    pub(crate) pending_voice_commands: Vec<::voice::ExecutableCommand>,
    pub focus_handle: FocusHandle,
    wallpaper: WorkspaceWallpaper,
    pub(crate) last_reflow_window_size: Option<(f32, f32)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppScreen {
    Launchpad,
    Canvas,
}

impl MultisoulApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let agent_hook_server = AgentHookServer::start();
        let mut runtime = Runtime::new();
        runtime.set_agent_hook_config(agent_hook_server.runtime_config());
        Self::assemble(cx, runtime, agent_hook_server, wallpaper_from_env())
    }

    /// 测试入口：注入 PTY 工厂，使 Cmd+N 走 TestPtyBackend 而非真实 PTY。
    /// 与 `new` 行为一致，仅在 `Runtime::new()` 后额外调用 `set_pty_factory`。
    pub fn new_with_pty_factory(cx: &mut Context<Self>, factory: runtime::PtyFactory) -> Self {
        let agent_hook_server = AgentHookServer::start();
        let mut runtime = Runtime::new();
        runtime.set_pty_factory(factory);
        runtime.set_agent_hook_config(agent_hook_server.runtime_config());
        Self::assemble(cx, runtime, agent_hook_server, wallpaper_from_env())
    }

    /// 当前 App 屏幕（Launchpad / Canvas）。供 e2e 测试从外部 crate 断言屏幕状态——
    /// `screen` 字段为 `pub(crate)`，跨 crate 读取必须经此访问器。
    pub fn screen(&self) -> AppScreen {
        self.screen
    }

    /// E2E 测试专用：直接切换到 Canvas 屏幕，**不**走 `OpenProject` →
    /// `enter_workspace_for_path` 路径。
    ///
    /// # 为什么需要这个入口
    /// 真实 `OpenProject` 流程调用 `cx.prompt_for_paths`，而 GPUI 的
    /// `TestPlatform::prompt_for_paths` 是 `unimplemented!()`（仅
    /// `prompt_for_new_path` 可被 `simulate_new_path_selection` 驱动）。此外
    /// `enter_workspace_for_path` 会用全新 `Runtime::new_for_workspace(root)`
    /// 替换旧 runtime，导致 `set_pty_factory` 注入的测试工厂丢失。本方法绕过
    /// 这两个问题：直接置 `screen = Canvas`，保留现有 runtime（含工厂），
    /// 不加载持久化快照、不恢复 session。fresh tempdir 场景下与真实流程行为等价。
    ///
    /// 生产路径**禁止**调用本方法；必须经 `OpenProject` action 进入 canvas。
    pub fn enter_canvas_for_test(&mut self, cx: &mut Context<Self>) {
        self.screen = AppScreen::Canvas;
        self.last_reflow_window_size = None;
        cx.notify();
    }

    /// `new` / `new_with_pty_factory` 共享的构造核心：启动后台帧循环并装配字段。
    /// 调用方负责准备 `Runtime`（含可选 PTY 工厂与 agent hook 配置）。
    fn assemble(
        cx: &mut Context<Self>,
        runtime: Runtime,
        agent_hook_server: AgentHookServer,
        wallpaper: WorkspaceWallpaper,
    ) -> Self {
        // 后台帧循环：每 ~12ms drain 一次（8–16ms 批处理窗口），dirty 才 repaint。
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(12))
                    .await;
                let go = this.update(cx, |app, cx| {
                    let runtime_changed = app.runtime.drain_all();
                    let agent_hook_changed =
                        app.agent_hook_server.drain_into_runtime(&mut app.runtime);
                    let runtime_events = app.runtime.take_events();
                    for event in &runtime_events {
                        tracing::debug!(?event, "runtime event");
                    }
                    let motion_changed = app.motion.tick(Duration::from_millis(12));
                    let persistence_changed = app.tick_persistence(Duration::from_millis(12));
                    let norma_pending = app.norma_server.has_pending();
                    if runtime_changed
                        || !runtime_events.is_empty()
                        || agent_hook_changed
                        || motion_changed
                        || persistence_changed
                        || norma_pending
                        || app.agent_hook_server.has_pending()
                    {
                        cx.notify();
                    }
                });
                if go.is_err() {
                    break; // 视图已释放
                }
            }
        })
        .detach();

        MultisoulApp {
            runtime,
            theme: Theme::dark(),
            interaction: InteractionState::default(),
            motion: MotionStore::from_env(),
            previews: PreviewStore::default(),
            master_layout: MasterLayoutState::default(),
            persistence: PersistenceScheduler::default(),
            settings_panel: SettingsPanelState::default(),
            norma_server: NormaServer::start(),
            agent_hook_server,
            screen: AppScreen::Launchpad,
            workspace_state: WorkspaceState::load_default(),
            voice: crate::app::voice::control::VoiceControlState::default(),
            pending_voice_commands: Vec::new(),
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
                spawn_focus::activate_spawned_node(&mut self.runtime, node_id);
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
                spawn_focus::activate_spawned_node(&mut self.runtime, node_id);
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

    fn on_open_project(&mut self, _: &OpenProject, _window: &mut Window, cx: &mut Context<Self>) {
        self.prompt_open_project(cx);
    }

    fn on_open_preview_file(
        &mut self,
        _: &OpenPreviewFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_create_preview() {
            self.workspace_state.error = Some("Open a project before previewing files.".into());
            cx.notify();
            return;
        }
        self.prompt_preview_file(window, cx);
    }

    pub(crate) fn remove_node_and_reflow(&mut self, node_id: NodeId, window: &mut Window) -> bool {
        let area = self.visible_canvas_rect(window);
        let zoom = self.runtime.document.viewport.zoom;
        if self.interaction.keyboard_selected_node == Some(node_id) {
            self.interaction.keyboard_selected_node = None;
        }
        self.interaction.clear_text_selection_for_node(node_id);
        let changed = close_node_and_update_layout_for_area(
            &mut self.runtime,
            &mut self.motion,
            &mut self.master_layout,
            node_id,
            area,
            zoom,
        );
        if changed {
            self.previews.remove(node_id);
            self.mark_canvas_dirty();
            self.mark_session_dirty();
        }
        changed
    }

    pub(crate) fn visible_canvas_rect(&self, window: &mut Window) -> CanvasRect {
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

    pub(crate) fn layout_gap_canvas(&self) -> f32 {
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

    pub(crate) fn apply_layout_for_current_mode(&mut self, window: &mut Window) {
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

    fn on_toolbar_preview_file(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.on_open_preview_file(&OpenPreviewFile, window, cx);
    }

    fn on_toolbar_canvas_scroll(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.interaction.canvas_scroll_enabled = !self.interaction.canvas_scroll_enabled;
        cx.notify();
    }
}
impl Focusable for MultisoulApp {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MultisoulApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.drain_norma_server(window, cx);

        let mut root = div()
            .key_context("MultisoulApp")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_new_terminal))
            .on_action(cx.listener(Self::on_new_claude_agent))
            .on_action(cx.listener(Self::on_new_codex_agent))
            .on_action(cx.listener(Self::on_copy_card_content))
            .on_action(cx.listener(Self::on_open_project))
            .on_action(cx.listener(Self::on_open_preview_file))
            .on_action(cx.listener(Self::on_exit_focus))
            .on_action(cx.listener(Self::on_reflow_layout))
            .on_action(cx.listener(Self::on_toggle_master_layout))
            .on_key_down(cx.listener(Self::on_key_down))
            .on_key_up(cx.listener(Self::on_key_up))
            .on_modifiers_changed(cx.listener(Self::on_modifiers_changed))
            .size_full()
            .relative()
            .bg(gpui::rgba(self.theme.graphite))
            .text_size(px(13.0))
            .child(crate::app::ime::TerminalImeElement::new(
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
                if let Some(modal) = self.render_voice_confirmation_modal(cx) {
                    root = root.child(modal);
                }
                if self.settings_panel.open {
                    root = root.child(self.render_settings_panel(window, cx));
                }
            }
        }

        root
    }
}

#[cfg(test)]
#[path = "../../tests/unit/app_tests.rs"]
mod tests;
