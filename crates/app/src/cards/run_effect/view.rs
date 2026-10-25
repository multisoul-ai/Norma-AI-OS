use std::time::{Duration, Instant};

use canvas::NodeId;
use gpui::{canvas, div, prelude::*, px, Context, Window};
use ui::Theme;

use crate::motion::run_style::{self, CardFrame, RunInput, RunMotion, RunPalette, RunStyle};

use super::paint_prims;

/// 运行特效 30fps：灵光与磁流体都是缓慢起伏，60fps 只会翻倍成本。
const RUN_EFFECT_FRAME_INTERVAL: Duration = Duration::from_millis(33);

#[derive(Clone, Copy)]
pub(crate) struct RunEffectPresentation {
    pub(crate) style: RunStyle,
    pub(crate) input: RunInput,
    pub(crate) visible: bool,
    pub(crate) reduce_motion: bool,
    pub(crate) theme: Theme,
    pub(crate) left_px: f32,
    pub(crate) top_px: f32,
    pub(crate) width_px: f32,
    pub(crate) height_px: f32,
    pub(crate) opacity: f32,
    pub(crate) zoom: f32,
    pub(crate) node_radius_px: f32,
    pub(crate) title_h_px: f32,
}

/// 一张 Agent 卡片的运行特效 Entity。与 `WorkingSnakeView` 同样自带显示帧驱动，
/// 每帧只 notify 自己，画布不随特效重建。
pub(crate) struct RunEffectView {
    node_id: NodeId,
    presentation: RunEffectPresentation,
    palette: RunPalette,
    motion: RunMotion,
    last_tick: Instant,
    frame_accumulator: Duration,
    frame_pending: bool,
}

impl RunEffectView {
    pub(crate) fn new(node_id: NodeId, presentation: RunEffectPresentation, now: Instant) -> Self {
        let palette = RunPalette::from_theme(presentation.theme);
        let mut motion = RunMotion::new(presentation.style, presentation.input, &palette);
        if presentation.reduce_motion {
            motion.tick(0.0, presentation.style, &palette, true);
        }
        Self {
            node_id,
            presentation,
            palette,
            motion,
            last_tick: now,
            frame_accumulator: Duration::ZERO,
            frame_pending: false,
        }
    }

    pub(crate) fn synchronize(&mut self, next: RunEffectPresentation, cx: &mut Context<Self>) {
        let previous = self.presentation;
        let mut changed = false;
        let palette = RunPalette::from_theme(next.theme);
        if palette != self.palette {
            self.palette = palette;
            changed = true;
        }
        if next.style != previous.style {
            self.motion = RunMotion::new(next.style, next.input, &self.palette);
            changed = true;
        }
        changed |= self.motion.set_input(next.input);
        // 刚打开减少动态效果时也要落到目标值，否则会停在过渡中间。
        if next.reduce_motion && (changed || !previous.reduce_motion) {
            // 减少动态效果：不出帧，状态变化直接落到目标值。
            self.motion.tick(0.0, next.style, &self.palette, true);
        }
        changed |= next.visible != previous.visible
            || next.reduce_motion != previous.reduce_motion
            || [
                (next.left_px, previous.left_px),
                (next.top_px, previous.top_px),
                (next.width_px, previous.width_px),
                (next.height_px, previous.height_px),
                (next.opacity, previous.opacity),
                (next.zoom, previous.zoom),
            ]
            .iter()
            .any(|(a, b)| (a - b).abs() > f32::EPSILON);
        self.presentation = next;
        if changed {
            cx.notify();
        }
    }

    pub(crate) fn is_active_animation(&self) -> bool {
        self.presentation.visible && self.motion.wants_frames(self.presentation.reduce_motion)
    }

    fn start_frame_driver_if_needed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.frame_pending || !self.is_active_animation() {
            return;
        }
        self.last_tick = cx.background_executor().now();
        self.frame_accumulator = Duration::ZERO;
        self.schedule_next_frame(window, cx);
    }

    fn schedule_next_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.frame_pending = true;
        cx.on_next_frame(window, |view, window, cx| {
            view.on_animation_frame(window, cx)
        });
    }

    fn on_animation_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.frame_pending = false;
        if !self.is_active_animation() {
            return;
        }
        let now = cx.background_executor().now();
        self.frame_accumulator += now.saturating_duration_since(self.last_tick);
        self.last_tick = now;
        if self.frame_accumulator >= RUN_EFFECT_FRAME_INTERVAL {
            let dt = std::mem::take(&mut self.frame_accumulator);
            let p = self.presentation;
            self.motion.tick(
                dt.as_secs_f32() * 1000.0,
                p.style,
                &self.palette,
                p.reduce_motion,
            );
            cx.notify();
        }
        self.schedule_next_frame(window, cx);
    }
}

impl gpui::Render for RunEffectView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.start_frame_driver_if_needed(window, cx);
        let p = self.presentation;
        let margin = p.style.margin() * p.zoom;
        let layer = div()
            .id(("run-effect-view", self.node_id))
            .absolute()
            .left(px(p.left_px - margin))
            .top(px(p.top_px - margin))
            .w(px(p.width_px + margin * 2.0))
            .h(px(p.height_px + margin * 2.0))
            .opacity(p.opacity);
        if !p.visible || p.style == RunStyle::Classic {
            return layer;
        }
        let frame = CardFrame {
            x: margin,
            y: margin,
            w: p.width_px,
            h: p.height_px,
            radius: p.node_radius_px,
            zoom: p.zoom,
            title_h: p.title_h_px,
        };
        let started = Instant::now();
        let prims = run_style::build(p.style, &self.motion, &frame, p.reduce_motion);
        let build_elapsed = started.elapsed();
        let node_id = self.node_id;
        layer.child(
            canvas(
                move |_bounds, _window, _cx| prims,
                move |bounds, prims, window, _cx| {
                    let paint_started = Instant::now();
                    paint_prims(&prims, bounds.origin, window);
                    crate::frame_diagnostics::RunEffectPaintReport {
                        node_id,
                        style: p.style.id(),
                        prim_count: prims.len(),
                        build_elapsed,
                        paint_elapsed: paint_started.elapsed(),
                    }
                    .log_if_enabled();
                },
            )
            .absolute()
            .size_full(),
        )
    }
}
