//! GPUI 主视图：持有 Runtime，装配画布/终端渲染，处理快捷键与帧级 drain。

use crate::assets::DEFAULT_WALLPAPER_ASSET;
use crate::canvas_view::{render_canvas, InteractionState};
use crate::cards::card_for;
use crate::cards::term::view::keystroke_to_input;
use crate::motion::MotionStore;
use canvas::{
    compute_responsive_grid_layout, AgentProfile, CanvasRect, LayoutInsets, NodeId, NodeType,
    ResponsiveGridConfig, Vec2, DEFAULT_CARD_ASPECT_RATIO,
};
use gpui::{
    actions, div, prelude::*, px, ClickEvent, Context, FocusHandle, Focusable, ImageSource,
    KeyDownEvent, Window,
};
use runtime::{route, InputEvent, InputMode, NodeInput, NodeResize, Route, Runtime};
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
        ExitFocus,
        ReflowLayout
    ]
);

const LAYOUT_GAP_SCREEN_PX: f32 = 16.0;
const LAYOUT_MAX_COLUMNS: usize = 3;
const LAYOUT_MIN_COLUMN_SCREEN_PX: f32 = 320.0;
const WORKSPACE_SAFE_LEFT_PX: f32 = 96.0;
const WORKSPACE_SAFE_TOP_PX: f32 = 86.0;
const WORKSPACE_SAFE_RIGHT_PX: f32 = 32.0;
const WORKSPACE_SAFE_BOTTOM_PX: f32 = 84.0;
const WALLPAPER_IMAGE_OPACITY: f32 = 1.0;
const WALLPAPER_SHADE_TOP_ALPHA: u8 = 0x00;
const WALLPAPER_SHADE_BOTTOM_ALPHA: u8 = 0x00;

pub struct MultisoulApp {
    pub runtime: Runtime,
    pub theme: Theme,
    pub interaction: InteractionState,
    pub(crate) motion: MotionStore,
    pub focus_handle: FocusHandle,
    wallpaper: WorkspaceWallpaper,
    last_reflow_window_size: Option<(f32, f32)>,
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
                    if runtime_changed || motion_changed {
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
            focus_handle: cx.focus_handle(),
            wallpaper,
            last_reflow_window_size: None,
        }
    }

    /// Cmd+N：在当前可见画布区域内新建 Terminal 节点，并触发三列平铺 reflow。
    fn on_new_terminal(&mut self, _: &NewTerminal, window: &mut Window, cx: &mut Context<Self>) {
        let bounds = self
            .visible_canvas_rect(window)
            .inset(self.layout_gap_canvas());
        let (cols, rows) =
            term_grid_size_for_card(NodeType::Term, bounds, self.runtime.document.viewport.zoom);
        match self.runtime.spawn_terminal(bounds, "/bin/zsh", cols, rows) {
            Ok(node_id) => {
                tracing::info!(?node_id, "spawned terminal node");
                self.runtime.selection.select_only(node_id);
                self.motion.spawn_node(node_id, bounds);
                self.reflow_tiled_nodes(window);
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
        self.spawn_agent_card(AgentProfile::Claude, window, cx);
    }

    fn on_new_codex_agent(
        &mut self,
        _: &NewCodexAgent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
                self.runtime.selection.select_only(node_id);
                self.motion.spawn_node(node_id, bounds);
                self.reflow_tiled_nodes(window);
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
        self.reflow_tiled_nodes(window);
        cx.notify();
    }

    pub(crate) fn remove_node_and_reflow(&mut self, node_id: NodeId, window: &mut Window) -> bool {
        let area = self.visible_canvas_rect(window);
        let zoom = self.runtime.document.viewport.zoom;
        close_node_and_reflow_for_area(&mut self.runtime, &mut self.motion, node_id, area, zoom)
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
        LAYOUT_GAP_SCREEN_PX / self.runtime.document.viewport.zoom.max(0.001)
    }

    fn reflow_tiled_nodes(&mut self, window: &mut Window) {
        let area = self.visible_canvas_rect(window);
        let zoom = self.runtime.document.viewport.zoom;
        apply_workspace_reflow_for_area(&mut self.runtime, &mut self.motion, area, zoom);
    }

    fn reflow_if_window_size_changed(&mut self, window: &mut Window) {
        let size = window.viewport_size();
        let current = (f32::from(size.width), f32::from(size.height));
        if self.last_reflow_window_size != Some(current) {
            self.last_reflow_window_size = Some(current);
            self.reflow_tiled_nodes(window);
        }
    }

    /// 键盘事件：维护 space_held；聚焦终端时把按键编码写入对应 PTY。
    fn on_key_down(&mut self, ev: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        // 维护空格按下状态（用于画布 pan）。注意：聚焦终端时空格应进终端，不做 pan 标记。
        let is_space = ev.keystroke.key == "space";
        if is_space && matches!(self.runtime.focus.mode(), InputMode::CanvasSelect) {
            self.interaction.space_held = true;
        }

        // 仅在节点聚焦模式下路由按键到 live node。
        if let InputMode::NodeFocused(node_id) = self.runtime.focus.mode() {
            if route(&self.runtime.focus.mode(), &InputEvent::Key)
                == Route::SendInputToNode(node_id)
            {
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

    fn on_toolbar_reflow(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.on_reflow_layout(&ReflowLayout, window, cx);
    }

    fn on_feedback_click(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        tracing::info!("feedback button clicked");
        cx.notify();
    }
}

impl Drop for MultisoulApp {
    fn drop(&mut self) {
        self.runtime.kill_all();
    }
}

impl Focusable for MultisoulApp {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MultisoulApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.reflow_if_window_size_changed(window);
        div()
            .key_context("MultisoulApp")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_new_terminal))
            .on_action(cx.listener(Self::on_new_claude_agent))
            .on_action(cx.listener(Self::on_new_codex_agent))
            .on_action(cx.listener(Self::on_exit_focus))
            .on_action(cx.listener(Self::on_reflow_layout))
            .on_key_down(cx.listener(Self::on_key_down))
            .on_key_up(cx.listener(Self::on_key_up))
            .size_full()
            .relative()
            .bg(gpui::rgba(self.theme.graphite))
            .text_size(px(13.0))
            .child(self.render_workspace_backdrop())
            .child(render_canvas(self, window, cx))
            .child(self.render_workspace_chrome(window, cx))
    }
}

fn workspace_reflow_config_for_zoom(zoom: f32) -> ResponsiveGridConfig {
    let zoom = zoom.max(0.001);
    ResponsiveGridConfig {
        max_columns: LAYOUT_MAX_COLUMNS,
        min_column_width: LAYOUT_MIN_COLUMN_SCREEN_PX / zoom,
        aspect_ratio: DEFAULT_CARD_ASPECT_RATIO,
        gap_canvas: LAYOUT_GAP_SCREEN_PX / zoom,
        safe_area: LayoutInsets::new(
            WORKSPACE_SAFE_LEFT_PX / zoom,
            WORKSPACE_SAFE_TOP_PX / zoom,
            WORKSPACE_SAFE_RIGHT_PX / zoom,
            WORKSPACE_SAFE_BOTTOM_PX / zoom,
        ),
    }
}

fn term_grid_size_for_card(node_type: NodeType, bounds: CanvasRect, zoom: f32) -> (u16, u16) {
    match card_for(node_type).resize(bounds, zoom) {
        Some(NodeResize::TermGrid { cols, rows }) => (cols, rows),
        None => (80, 24),
    }
}

fn apply_workspace_reflow_for_area(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    area: CanvasRect,
    zoom: f32,
) {
    let ordered_ids: Vec<_> = runtime
        .document
        .nodes()
        .iter()
        .map(|node| node.id)
        .collect();
    let assignments =
        compute_responsive_grid_layout(&ordered_ids, area, workspace_reflow_config_for_zoom(zoom));

    for assignment in assignments {
        let node_type = runtime.document.node(assignment.node_id).map(|node| {
            motion.ensure_node(assignment.node_id, node.bounds);
            node.kind.node_type()
        });
        runtime
            .document
            .set_node_bounds(assignment.node_id, assignment.bounds);
        motion.set_layout_target(assignment.node_id, assignment.bounds);

        if let Some(resize) =
            node_type.and_then(|node_type| card_for(node_type).resize(assignment.bounds, zoom))
        {
            let _ = runtime.resize_node(assignment.node_id, resize);
        }
    }
}

fn close_node_and_reflow_for_area(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    node_id: NodeId,
    area: CanvasRect,
    zoom: f32,
) -> bool {
    if !runtime.remove_node(node_id) {
        return false;
    }

    motion.clear_transient();
    motion.retain_nodes(runtime.document.nodes().iter().map(|node| node.id));
    apply_workspace_reflow_for_area(runtime, motion, area, zoom);
    true
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
