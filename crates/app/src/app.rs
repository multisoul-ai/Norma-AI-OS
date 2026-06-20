//! GPUI 主视图：持有 Runtime，装配画布/终端渲染，处理快捷键与帧级 drain。

use crate::canvas_view::{render_canvas, InteractionState, TITLE_BAR_H};
use crate::terminal_view::{keystroke_to_input, TERM_CELL_PX, TERM_LINE_PX, TERM_PADDING_PX};
use canvas::{compute_instant_bsp_layout, CanvasNodeKind, CanvasRect, InstantBspConfig, Vec2};
use gpui::{actions, div, prelude::*, px, Context, FocusHandle, Focusable, KeyDownEvent, Window};
use runtime::{route, InputEvent, InputMode, Route, Runtime};
use std::time::Duration;
use ui::Theme;

actions!(multisoul, [NewTerminal, ExitFocus, ReflowLayout]);

const LAYOUT_GAP_SCREEN_PX: f32 = 8.0;
const MIN_TERMINAL_COLS: u16 = 20;
const MIN_TERMINAL_ROWS: u16 = 3;

pub struct MultisoulApp {
    pub runtime: Runtime,
    pub theme: Theme,
    pub interaction: InteractionState,
    pub focus_handle: FocusHandle,
    last_reflow_window_size: Option<(f32, f32)>,
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
                    if app.runtime.drain_all() {
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
            runtime: Runtime::new(),
            theme: Theme::default(),
            interaction: InteractionState::default(),
            focus_handle: cx.focus_handle(),
            last_reflow_window_size: None,
        }
    }

    /// Cmd+N：在当前可见画布区域内新建 Terminal 节点，并触发即时 BSP reflow。
    fn on_new_terminal(&mut self, _: &NewTerminal, window: &mut Window, cx: &mut Context<Self>) {
        let bounds = self
            .visible_canvas_rect(window)
            .inset(self.layout_gap_canvas());
        let (cols, rows) = self.terminal_grid_size_for_bounds(bounds);
        match self.runtime.spawn_terminal(bounds, "/bin/zsh", cols, rows) {
            Ok(node_id) => {
                tracing::info!(?node_id, "spawned terminal node");
                self.runtime.selection.select_only(node_id);
                self.resize_terminal_for_node(node_id, cols, rows);
                self.reflow_visible_nodes(window);
            }
            Err(e) => {
                // SPEC §8：spawn 失败不 panic，仅 error 日志（节点内错误占位由 Task 15 渲染）。
                tracing::error!(error = %e, "failed to spawn terminal");
            }
        }
        cx.notify();
    }

    fn on_exit_focus(&mut self, _: &ExitFocus, _window: &mut Window, cx: &mut Context<Self>) {
        self.runtime.focus.exit_to_canvas();
        cx.notify();
    }

    fn on_reflow_layout(&mut self, _: &ReflowLayout, window: &mut Window, cx: &mut Context<Self>) {
        self.reflow_visible_nodes(window);
        cx.notify();
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

    fn reflow_visible_nodes(&mut self, window: &mut Window) {
        let area = self.visible_canvas_rect(window);
        let ordered_ids: Vec<_> = self
            .runtime
            .document
            .nodes()
            .iter()
            .filter(|node| node.bounds.intersects(area))
            .map(|node| node.id)
            .collect();

        let assignments = compute_instant_bsp_layout(
            &ordered_ids,
            area,
            InstantBspConfig {
                gap_canvas: self.layout_gap_canvas(),
            },
        );

        for assignment in assignments {
            let kind = self
                .runtime
                .document
                .node(assignment.node_id)
                .map(|node| node.kind);
            self.runtime
                .document
                .set_node_bounds(assignment.node_id, assignment.bounds);
            if matches!(kind, Some(CanvasNodeKind::Terminal(_))) {
                let (cols, rows) = self.terminal_grid_size_for_bounds(assignment.bounds);
                self.resize_terminal_for_node(assignment.node_id, cols, rows);
            }
        }
    }

    fn terminal_grid_size_for_bounds(&self, bounds: CanvasRect) -> (u16, u16) {
        let zoom = self.runtime.document.viewport.zoom.max(0.001);
        let screen_w = (bounds.size.x * zoom).max(0.0);
        let screen_h = (bounds.size.y * zoom).max(0.0);
        let content_w = (screen_w - TERM_PADDING_PX * 2.0).max(0.0);
        let content_h = (screen_h - TITLE_BAR_H * zoom - TERM_PADDING_PX * 2.0).max(0.0);
        let cols = clamp_terminal_dimension(content_w / (TERM_CELL_PX * zoom), MIN_TERMINAL_COLS);
        let rows = clamp_terminal_dimension(content_h / (TERM_LINE_PX * zoom), MIN_TERMINAL_ROWS);
        (cols, rows)
    }

    fn resize_terminal_for_node(&mut self, node_id: canvas::NodeId, cols: u16, rows: u16) {
        let Some(CanvasNodeKind::Terminal(r)) =
            self.runtime.document.node(node_id).map(|node| node.kind)
        else {
            return;
        };
        if let Some(session) = self.runtime.session_mut(r.session_id) {
            session.resize(cols, rows);
        }
    }

    fn reflow_if_window_size_changed(&mut self, window: &mut Window) {
        let size = window.viewport_size();
        let current = (f32::from(size.width), f32::from(size.height));
        if self.last_reflow_window_size != Some(current) {
            self.last_reflow_window_size = Some(current);
            self.reflow_visible_nodes(window);
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
                    if let Some(canvas::CanvasNodeKind::Terminal(r)) =
                        self.runtime.document.node(node_id).map(|n| n.kind)
                    {
                        let sid = r.session_id;
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
            .on_action(cx.listener(Self::on_exit_focus))
            .on_action(cx.listener(Self::on_reflow_layout))
            .on_key_down(cx.listener(Self::on_key_down))
            .on_key_up(cx.listener(Self::on_key_up))
            .size_full()
            .bg(gpui::rgba(self.theme.canvas_from))
            .text_size(px(13.0))
            .child(render_canvas(self, window, cx))
    }
}

fn clamp_terminal_dimension(raw: f32, min: u16) -> u16 {
    raw.floor().clamp(f32::from(min), f32::from(u16::MAX)) as u16
}
