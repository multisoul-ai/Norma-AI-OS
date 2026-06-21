//! GPUI 主视图：持有 Runtime，装配画布/终端渲染，处理快捷键与帧级 drain。

use crate::assets::DEFAULT_WALLPAPER_ASSET;
use crate::canvas_view::{render_canvas, InteractionState, TITLE_BAR_H};
use crate::motion::MotionStore;
use crate::terminal_view::{keystroke_to_input, TERM_CELL_PX, TERM_LINE_PX, TERM_PADDING_PX};
use canvas::{
    compute_responsive_grid_layout, AgentProfile, CanvasNodeKind, CanvasRect, LayoutInsets, NodeId,
    ResponsiveGridConfig, Vec2, DEFAULT_CARD_ASPECT_RATIO,
};
use gpui::{
    actions, div, img, linear_color_stop, linear_gradient, point, prelude::*, px, rgba, BoxShadow,
    ClickEvent, Context, Div, FocusHandle, Focusable, ImageSource, KeyDownEvent, ObjectFit, Window,
};
use runtime::{route, InputEvent, InputMode, Route, Runtime};
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;
use ui::Theme;

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
const MIN_TERMINAL_COLS: u16 = 20;
const MIN_TERMINAL_ROWS: u16 = 3;
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
        let (cols, rows) = self.terminal_grid_size_for_bounds(bounds);
        match self.runtime.spawn_terminal(bounds, "/bin/zsh", cols, rows) {
            Ok(node_id) => {
                tracing::info!(?node_id, "spawned terminal node");
                self.runtime.selection.select_only(node_id);
                self.motion.spawn_node(node_id, bounds);
                self.resize_terminal_for_node(node_id, cols, rows);
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
        let (cols, rows) = self.terminal_grid_size_for_bounds(bounds);
        match self.runtime.spawn_agent(profile, bounds, cols, rows) {
            Ok(node_id) => {
                tracing::info!(?node_id, ?profile, "spawned agent node");
                self.runtime.selection.select_only(node_id);
                self.motion.spawn_node(node_id, bounds);
                self.resize_terminal_for_node(node_id, cols, rows);
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

    fn terminal_grid_size_for_bounds(&self, bounds: CanvasRect) -> (u16, u16) {
        terminal_grid_size_for_bounds_at_zoom(bounds, self.runtime.document.viewport.zoom)
    }

    fn resize_terminal_for_node(&mut self, node_id: canvas::NodeId, cols: u16, rows: u16) {
        let Some(session_id) = self
            .runtime
            .document
            .node(node_id)
            .and_then(|node| node.kind.terminal_session_id())
        else {
            return;
        };
        if let Some(session) = self.runtime.session_mut(session_id) {
            session.resize(cols, rows);
        }
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

        // 仅在终端聚焦模式下路由按键到 PTY。
        if let InputMode::TerminalFocused(node_id) = self.runtime.focus.mode() {
            if route(&self.runtime.focus.mode(), &InputEvent::Key)
                == Route::SendKeyToTerminal(node_id)
            {
                if let Some(input) = keystroke_to_input(&ev.keystroke) {
                    // node_id → session_id
                    if let Some(sid) = self
                        .runtime
                        .document
                        .node(node_id)
                        .and_then(|node| node.kind.terminal_session_id())
                    {
                        if let Some(session) = self.runtime.session_mut(sid) {
                            if let Err(e) = session.write_input(&input) {
                                tracing::error!(error = %e, "write_input failed");
                            }
                        }
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

    fn render_workspace_backdrop(&self) -> Div {
        let theme = self.theme;
        let mut backdrop = div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .overflow_hidden()
            .bg(linear_gradient(
                150.0,
                linear_color_stop(rgba(theme.canvas_from), 0.0),
                linear_color_stop(rgba(theme.graphite), 1.0),
            ));

        backdrop = backdrop.child(
            img(self.wallpaper.image_source())
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .size_full()
                .object_fit(ObjectFit::Cover)
                .opacity(WALLPAPER_IMAGE_OPACITY),
        );

        backdrop
            .child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .right(px(0.0))
                    .bottom(px(0.0))
                    .bg(linear_gradient(
                        180.0,
                        linear_color_stop(
                            rgba(with_alpha(theme.graphite, WALLPAPER_SHADE_TOP_ALPHA)),
                            0.0,
                        ),
                        linear_color_stop(
                            rgba(with_alpha(theme.graphite, WALLPAPER_SHADE_BOTTOM_ALPHA)),
                            1.0,
                        ),
                    )),
            )
            .child(
                div()
                    .absolute()
                    .left(px(-160.0))
                    .top(px(54.0))
                    .w(px(520.0))
                    .h(px(520.0))
                    .rounded(px(260.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.accent_cyan, 0x24))),
            )
            .child(
                div()
                    .absolute()
                    .right(px(-180.0))
                    .bottom(px(-120.0))
                    .w(px(620.0))
                    .h(px(620.0))
                    .rounded(px(310.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.accent_orange, 0x2e))),
            )
            .child(
                div()
                    .absolute()
                    .left(px(90.0))
                    .right(px(90.0))
                    .bottom(px(84.0))
                    .h(px(1.0))
                    .bg(rgba(with_alpha(theme.accent_cyan, 0x18))),
            )
    }

    fn render_workspace_chrome(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let theme = self.theme;
        let win = window.viewport_size();
        let win_w = f32::from(win.width);
        let bottom_w = (win_w - 64.0).clamp(260.0, 360.0);
        let bottom_left = (win_w - bottom_w) * 0.5;

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .child(
                div()
                    .absolute()
                    .top(px(30.0))
                    .left(px((win_w - 312.0) * 0.5))
                    .w(px(312.0))
                    .h(px(46.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .px(px(12.0))
                    .rounded(px(23.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.edge_highlight, 0x34)))
                    .shadow(chrome_shadow(theme))
                    .bg(rgba(with_alpha(theme.panel, 0xdc)))
                    .text_color(rgba(theme.text_primary))
                    .text_size(px(12.0))
                    .child(chrome_button(theme, "v", false))
                    .child(div().px(px(8.0)).child("CANVAS"))
                    .child(
                        chrome_button(theme, "+", true)
                            .id("toolbar-new-terminal")
                            .on_click(cx.listener(Self::on_toolbar_new_terminal)),
                    )
                    .child(
                        chrome_button(theme, "CL", false)
                            .id("toolbar-new-claude-agent")
                            .on_click(cx.listener(Self::on_toolbar_new_claude_agent)),
                    )
                    .child(
                        chrome_button(theme, "CX", false)
                            .id("toolbar-new-codex-agent")
                            .on_click(cx.listener(Self::on_toolbar_new_codex_agent)),
                    )
                    .child(
                        chrome_button(theme, "[]", false)
                            .id("toolbar-reflow")
                            .on_click(cx.listener(Self::on_toolbar_reflow)),
                    )
                    .child(chrome_button(theme, "*", false)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(24.0))
                    .top(px(178.0))
                    .w(px(46.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .p(px(8.0))
                    .rounded(px(23.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.edge_highlight, 0x2f)))
                    .shadow(chrome_shadow(theme))
                    .bg(rgba(with_alpha(theme.panel, 0xd0)))
                    .child(chrome_button(theme, ">", true))
                    .child(chrome_button(theme, "H", false))
                    .child(chrome_button(theme, "[]", false))
                    .child(chrome_button(theme, "<>", false))
                    .child(chrome_button(theme, "O", false))
                    .child(chrome_button(theme, "T", false))
                    .child(chrome_button(theme, "AI", false))
                    .child(chrome_button(theme, "{}", false)),
            )
            .child(
                div()
                    .absolute()
                    .right(px(24.0))
                    .top(px(34.0))
                    .h(px(38.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px(px(14.0))
                    .rounded(px(19.0))
                    .shadow(chrome_shadow(theme))
                    .bg(rgba(theme.accent_orange))
                    .text_color(rgba(0xffffffff))
                    .text_size(px(12.0))
                    .child(chrome_button(theme, "!", true))
                    .child("Feedback")
                    .id("workspace-feedback")
                    .on_click(cx.listener(Self::on_feedback_click)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(bottom_left))
                    .bottom(px(22.0))
                    .w(px(bottom_w))
                    .h(px(42.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px(px(18.0))
                    .rounded(px(21.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.edge_highlight, 0x2d)))
                    .shadow(chrome_shadow(theme))
                    .bg(rgba(with_alpha(theme.panel, 0xe5)))
                    .text_color(rgba(theme.text_primary))
                    .text_size(px(13.0))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(18.0))
                                    .h(px(18.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(rgba(theme.accent_orange))
                                    .child("|||"),
                            )
                            .child("Listening..."),
                    )
                    .child(
                        div()
                            .text_color(rgba(theme.accent_orange))
                            .child("........"),
                    ),
            )
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

fn clamp_terminal_dimension(raw: f32, min: u16) -> u16 {
    raw.floor().clamp(f32::from(min), f32::from(u16::MAX)) as u16
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

fn terminal_grid_size_for_bounds_at_zoom(bounds: CanvasRect, zoom: f32) -> (u16, u16) {
    let zoom = zoom.max(0.001);
    let screen_w = (bounds.size.x * zoom).max(0.0);
    let screen_h = (bounds.size.y * zoom).max(0.0);
    let content_w = (screen_w - TERM_PADDING_PX * 2.0).max(0.0);
    let content_h = (screen_h - TITLE_BAR_H * zoom - TERM_PADDING_PX * 2.0).max(0.0);
    let cols = clamp_terminal_dimension(content_w / (TERM_CELL_PX * zoom), MIN_TERMINAL_COLS);
    let rows = clamp_terminal_dimension(content_h / (TERM_LINE_PX * zoom), MIN_TERMINAL_ROWS);
    (cols, rows)
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
        let kind = runtime.document.node(assignment.node_id).map(|node| {
            motion.ensure_node(assignment.node_id, node.bounds);
            node.kind
        });
        runtime
            .document
            .set_node_bounds(assignment.node_id, assignment.bounds);
        motion.set_layout_target(assignment.node_id, assignment.bounds);

        let Some(session_id) = kind.and_then(CanvasNodeKind::terminal_session_id) else {
            continue;
        };
        let Some(session) = runtime.session_mut(session_id) else {
            continue;
        };
        let (cols, rows) = terminal_grid_size_for_bounds_at_zoom(assignment.bounds, zoom);
        session.resize(cols, rows);
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

fn chrome_shadow(theme: Theme) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: rgba(theme.shadow_color).into(),
        offset: point(px(0.0), px(18.0)),
        blur_radius: px(36.0),
        spread_radius: px(-18.0),
    }]
}

fn chrome_button(theme: Theme, label: &'static str, active: bool) -> Div {
    let fill = if active {
        with_alpha(theme.accent_orange, 0x36)
    } else {
        with_alpha(theme.edge_highlight, 0x08)
    };
    let stroke = if active {
        with_alpha(theme.accent_orange, 0x78)
    } else {
        with_alpha(theme.edge_highlight, 0x20)
    };
    let text = if active {
        theme.accent_orange
    } else {
        theme.text_primary
    };

    div()
        .w(px(28.0))
        .h(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(stroke))
        .bg(rgba(fill))
        .text_color(rgba(text))
        .text_size(px(11.0))
        .child(label)
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}

#[cfg(test)]
mod tests {
    use super::*;
    use canvas::RectNode;

    /// 默认壁纸：未设置 env 时应使用打包进程序的内置画布壁纸。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   env_value = None（模拟未设置 MULTISOUL_WALLPAPER）
    ///   default asset path = "wallpapers/1331865.png"（应用启动时交给 GPUI asset source 加载）
    ///   fallback count = 1（只有一个默认壁纸候选，避免空背景）
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. wallpaper_from_env_value(None) → 不存在用户覆盖路径
    ///   2. 选择 WorkspaceWallpaper::Embedded(DEFAULT_WALLPAPER_ASSET)
    ///
    /// 预期结果：
    ///   - 正断言：返回内置 asset 路径 wallpapers/1331865.png
    ///   - 负断言：不返回 File 路径，避免打包后依赖开发机外部文件
    #[test]
    fn test_default_wallpaper_uses_embedded_asset_when_env_missing() {
        let wallpaper = wallpaper_from_env_value(None);

        assert_eq!(
            wallpaper,
            WorkspaceWallpaper::Embedded(DEFAULT_WALLPAPER_ASSET),
            "missing MULTISOUL_WALLPAPER should use the embedded default asset"
        );
        assert!(
            !matches!(wallpaper, WorkspaceWallpaper::File(_)),
            "missing MULTISOUL_WALLPAPER must not produce a file path dependency"
        );
    }

    /// 用户壁纸覆盖：env 指向可读文件时应优先使用用户指定路径。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   temp file path = std::env::temp_dir()/multisoul-wallpaper-override.png
    ///   temp file bytes = 3（只用于 is_file 判定，不参与图片解码）
    ///   env_value = Some(temp file path)
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 创建临时普通文件 → path.is_file() = true
    ///   2. wallpaper_from_env_value(Some(path)) → 识别为用户覆盖
    ///   3. 删除临时文件 → 测试清理
    ///
    /// 预期结果：
    ///   - 正断言：返回 File(temp file path)
    ///   - 负断言：不返回 Embedded 默认壁纸，证明覆盖生效
    #[test]
    fn test_wallpaper_env_file_overrides_embedded_default() {
        let path = std::env::temp_dir().join(format!(
            "multisoul-wallpaper-override-{}.png",
            std::process::id()
        ));
        std::fs::write(&path, [1_u8, 2, 3]).expect("test should create temp wallpaper file");

        let wallpaper = wallpaper_from_env_value(Some(path.clone().into_os_string()));

        std::fs::remove_file(&path).expect("test should remove temp wallpaper file");
        assert_eq!(
            wallpaper,
            WorkspaceWallpaper::File(path),
            "readable MULTISOUL_WALLPAPER file should override the embedded default"
        );
        assert_ne!(
            wallpaper,
            WorkspaceWallpaper::Embedded(DEFAULT_WALLPAPER_ASSET),
            "readable MULTISOUL_WALLPAPER must not fall back to the embedded asset"
        );
    }

    /// 无效用户壁纸：env 指向不存在的文件时应回退到内置画布壁纸。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   missing path = std::env::temp_dir()/multisoul-missing-wallpaper.png
    ///   path.is_file() = false（不存在，因此不能作为图片资源）
    ///   fallback count = 1（内置默认壁纸）
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 确保 missing path 不存在 → path.is_file() = false
    ///   2. wallpaper_from_env_value(Some(missing path)) → 触发回退
    ///
    /// 预期结果：
    ///   - 正断言：返回 Embedded(DEFAULT_WALLPAPER_ASSET)
    ///   - 负断言：不返回 File(missing path)，避免渲染层加载坏路径
    #[test]
    fn test_invalid_wallpaper_env_falls_back_to_embedded_default() {
        let path = std::env::temp_dir().join(format!(
            "multisoul-missing-wallpaper-{}.png",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let wallpaper = wallpaper_from_env_value(Some(path.clone().into_os_string()));

        assert_eq!(
            wallpaper,
            WorkspaceWallpaper::Embedded(DEFAULT_WALLPAPER_ASSET),
            "invalid MULTISOUL_WALLPAPER should fall back to the embedded default asset"
        );
        assert_ne!(
            wallpaper,
            WorkspaceWallpaper::File(path),
            "invalid MULTISOUL_WALLPAPER must not be kept as a file-backed wallpaper"
        );
    }

    /// 壁纸亮度：默认画布壁纸应按原图亮度显示，不额外压暗。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   image opacity = 1.0（原图亮度需要完整不透明绘制，0.78 会混入深色底色）
    ///   top shade alpha = 0x00 / 0xff = 0%（顶部遮罩不压暗）
    ///   bottom shade alpha = 0x00 / 0xff = 0%（底部遮罩不压暗；旧值 0xd8 ≈ 85% 会明显变暗）
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 渲染壁纸图片 → 使用 WALLPAPER_IMAGE_OPACITY
    ///   2. 渲染遮罩层 → 使用 WALLPAPER_SHADE_TOP_ALPHA 与 WALLPAPER_SHADE_BOTTOM_ALPHA
    ///   3. 两个 alpha 都为 0 → 遮罩透明，图片亮度不被 graphite 混色改变
    ///
    /// 预期结果：
    ///   - 正断言：图片 opacity 为 1.0
    ///   - 正断言：顶部遮罩 alpha 为 0x00
    ///   - 正断言：底部遮罩 alpha 为 0x00
    ///   - 负断言：底部遮罩不再是旧的 0xd8 暗化值
    #[test]
    fn test_wallpaper_brightness_uses_original_image_luminance() {
        assert_eq!(
            WALLPAPER_IMAGE_OPACITY, 1.0,
            "wallpaper image should render fully opaque to preserve original brightness"
        );
        assert_eq!(
            WALLPAPER_SHADE_TOP_ALPHA, 0x00,
            "wallpaper top shade alpha should be transparent"
        );
        assert_eq!(
            WALLPAPER_SHADE_BOTTOM_ALPHA, 0x00,
            "wallpaper bottom shade alpha should be transparent"
        );
        assert_ne!(
            WALLPAPER_SHADE_BOTTOM_ALPHA, 0xd8,
            "wallpaper bottom shade must not keep the old darkening alpha"
        );
    }

    /// 删除中间卡片后，剩余卡片应立即按两列布局重排。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   area = origin(0,0), size(1200x900)
    ///   zoom = 1.0
    ///   safe area = left 96, top 86, right 32, bottom 84
    ///   usable width = 1200 - 96 - 32 = 1072
    ///   gap = 16
    ///   三节点布局：tile_width = (1072 - 16*(3+1)) / 3 = 336，第三列 x = 96 + 16 + 2*(336+16) = 816
    ///   删除中间节点后：tile_width = (1072 - 16*(2+1)) / 2 = 512，第二列 x = 96 + 16 + 1*(512+16) = 640
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 创建 A/B/C 三张普通卡片，并手动放到三列布局位置
    ///   2. close_node_and_reflow_for_area(B) → 删除 B，并用剩余顺序 [A,C] 重新计算布局
    ///   3. 查询 A/B/C 的 document bounds
    ///
    /// 预期结果：
    ///   - 正断言：B 被删除，返回 true
    ///   - 正断言：A 仍位于两列布局第一列
    ///   - 正断言：C 移到两列布局第二列 x=640
    ///   - 负断言：C 不应停留在三列布局第三列 x=816
    #[test]
    fn test_close_node_reflows_remaining_cards_into_two_column_layout() {
        let mut runtime = Runtime::new();
        let mut motion = MotionStore::new(true);
        let area = CanvasRect::new(0.0, 0.0, 1200.0, 900.0);
        let zoom = 1.0;

        let a = runtime.document.add_node(
            CanvasNodeKind::Rect(RectNode { color: 0x4080ff }),
            CanvasRect::new(0.0, 0.0, 10.0, 10.0),
        );
        let b = runtime.document.add_node(
            CanvasNodeKind::Rect(RectNode { color: 0xff8040 }),
            CanvasRect::new(0.0, 0.0, 10.0, 10.0),
        );
        let c = runtime.document.add_node(
            CanvasNodeKind::Rect(RectNode { color: 0x35d07f }),
            CanvasRect::new(0.0, 0.0, 10.0, 10.0),
        );

        let three_column = compute_responsive_grid_layout(
            &[a, b, c],
            area,
            workspace_reflow_config_for_zoom(zoom),
        );
        runtime.document.set_node_bounds(a, three_column[0].bounds);
        runtime.document.set_node_bounds(b, three_column[1].bounds);
        runtime.document.set_node_bounds(c, three_column[2].bounds);
        let c_three_column_bounds = three_column[2].bounds;

        let removed = close_node_and_reflow_for_area(&mut runtime, &mut motion, b, area, zoom);

        let two_column =
            compute_responsive_grid_layout(&[a, c], area, workspace_reflow_config_for_zoom(zoom));
        let a_after = runtime
            .document
            .node(a)
            .expect("A 应在删除 B 后继续存在")
            .bounds;
        let c_after = runtime
            .document
            .node(c)
            .expect("C 应在删除 B 后继续存在")
            .bounds;

        assert!(
            removed,
            "close_node_and_reflow_for_area(B) 应返回 true，表示确实删除了中间卡片"
        );
        assert!(
            runtime.document.node(b).is_none(),
            "B 应从 document 删除，不能继续参与渲染或布局"
        );
        assert_eq!(
            a_after, two_column[0].bounds,
            "A 应按删除后的两列布局占据第一列，而不是保留旧三列尺寸"
        );
        assert_eq!(
            c_after, two_column[1].bounds,
            "C 应按删除后的两列布局移动到第二列 x=640"
        );
        assert_ne!(
            c_after, c_three_column_bounds,
            "C 不应停留在删除前的三列第三列 x=816，否则说明删除没有触发重排"
        );
    }
}
