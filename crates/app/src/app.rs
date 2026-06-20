//! GPUI 主视图：持有 Runtime，装配画布/终端渲染，处理快捷键与帧级 drain。

use crate::canvas_view::{render_canvas, InteractionState};
use gpui::{
    actions, div, prelude::*, px, Context, FocusHandle, Focusable, Window,
};
use runtime::Runtime;
use std::time::Duration;
use ui::Theme;

actions!(multisoul, [NewTerminal, ExitFocus]);

pub struct MultisoulApp {
    pub runtime: Runtime,
    pub theme: Theme,
    pub interaction: InteractionState,
    pub focus_handle: FocusHandle,
}

impl MultisoulApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // 后台帧循环：每 ~12ms drain 一次（8–16ms 批处理窗口），dirty 才 repaint。
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(12)).await;
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
            theme: Theme::dark(),
            interaction: InteractionState::default(),
            focus_handle: cx.focus_handle(),
        }
    }

    /// Cmd+N：在画布可见区域中心新建一个 Terminal 节点。
    fn on_new_terminal(&mut self, _: &NewTerminal, _window: &mut Window, cx: &mut Context<Self>) {
        use canvas::CanvasRect;
        // 节点尺寸 640x400（画布坐标）；位置先用一个递增偏移避免重叠。
        let n = self.runtime.document.nodes().len() as f32;
        let x = 80.0 + n * 32.0;
        let y = 80.0 + n * 32.0;
        let bounds = CanvasRect::new(x, y, 640.0, 400.0);
        // 80 列 × 24 行是终端常规初始尺寸。
        match self.runtime.spawn_terminal(bounds, "/bin/zsh", 80, 24) {
            Ok(node_id) => {
                tracing::info!(?node_id, "spawned terminal node");
                self.runtime.selection.select_only(node_id);
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
}

impl Focusable for MultisoulApp {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MultisoulApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("MultisoulApp")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_new_terminal))
            .on_action(cx.listener(Self::on_exit_focus))
            .size_full()
            .bg(gpui::rgb(self.theme.canvas_bg))
            .text_size(px(13.0))
            .child(render_canvas(self, window, cx))
    }
}
