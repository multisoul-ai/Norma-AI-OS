use std::time::{Duration, Instant};

use canvas::NodeId;
use gpui::{div, prelude::*, Context, Window};
use ui::Theme;

use crate::motion::scalar::AnimatedScalar;
use crate::motion::tokens::MotionTokens;
use crate::motion::working::{resolve_working_snake, WORKING_SNAKE_PERIOD_SECS};

use super::render_working_snake_border;

const WORKING_SNAKE_FRAME_INTERVAL: Duration = Duration::from_nanos(16_666_667);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WorkingSnakeFrameOutcome {
    pub(crate) deferred: bool,
    pub(crate) state_advanced: bool,
    pub(crate) repaint_notified: bool,
    pub(crate) next_frame_requested: bool,
}

#[cfg(feature = "e2e-test-support")]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WorkingSnakeSnapshot {
    pub(crate) active: bool,
    pub(crate) next_frame_requested: bool,
    pub(crate) frame_pending: bool,
    pub(crate) phase: f32,
    pub(crate) render_count: usize,
}

/// Agent 工作态边框的局部 GPUI Entity。
///
/// 它持有自己的显示帧回调和 fade/phase，因此每帧只 notify 当前 Entity；父级
/// CanvasSurfaceView 只在节点、布局或业务状态改变时重建。
pub(crate) struct WorkingSnakeView {
    node_id: NodeId,
    active: bool,
    visible: bool,
    window_active: bool,
    reduce_motion: bool,
    theme: Theme,
    left_px: f32,
    top_px: f32,
    width_px: f32,
    height_px: f32,
    opacity: f32,
    zoom: f32,
    node_radius_px: f32,
    fade: AnimatedScalar,
    started_at: Option<Instant>,
    last_tick: Instant,
    frame_accumulator: Duration,
    frame_pending: bool,
    frame_generation: u64,
    #[cfg(feature = "e2e-test-support")]
    frame_loop_paused_for_e2e: bool,
    #[cfg(feature = "e2e-test-support")]
    render_count_for_e2e: usize,
}

impl WorkingSnakeView {
    pub(crate) fn new(
        node_id: NodeId,
        active: bool,
        visible: bool,
        window_active: bool,
        reduce_motion: bool,
        theme: Theme,
        left_px: f32,
        top_px: f32,
        width_px: f32,
        height_px: f32,
        opacity: f32,
        zoom: f32,
        node_radius_px: f32,
        now: Instant,
    ) -> Self {
        let running = active && visible;
        let fade = if reduce_motion && running {
            AnimatedScalar::on()
        } else {
            let mut fade = AnimatedScalar::off();
            fade.set_bool(running);
            fade
        };
        Self {
            node_id,
            active,
            visible,
            window_active,
            reduce_motion,
            theme,
            left_px,
            top_px,
            width_px,
            height_px,
            opacity,
            zoom,
            node_radius_px,
            fade,
            started_at: running.then_some(now),
            last_tick: now,
            frame_accumulator: Duration::ZERO,
            frame_pending: false,
            frame_generation: 0,
            #[cfg(feature = "e2e-test-support")]
            frame_loop_paused_for_e2e: false,
            #[cfg(feature = "e2e-test-support")]
            render_count_for_e2e: 0,
        }
    }

    pub(crate) fn set_window_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.window_active == active {
            return;
        }
        self.window_active = active;
        self.frame_generation = self.frame_generation.wrapping_add(1);
        self.frame_pending = false;
        self.last_tick = cx.background_executor().now();
        self.frame_accumulator = Duration::ZERO;
        cx.notify();
    }

    pub(crate) fn synchronize(
        &mut self,
        active: bool,
        visible: bool,
        reduce_motion: bool,
        theme: Theme,
        left_px: f32,
        top_px: f32,
        width_px: f32,
        height_px: f32,
        opacity: f32,
        zoom: f32,
        node_radius_px: f32,
        now: Instant,
        cx: &mut Context<Self>,
    ) {
        let was_running = self.active && self.visible;
        let running = active && visible;
        let visual_changed = self.reduce_motion != reduce_motion
            || self.theme.accent_cyan != theme.accent_cyan
            || self.theme.accent_violet != theme.accent_violet
            || (self.left_px - left_px).abs() > f32::EPSILON
            || (self.top_px - top_px).abs() > f32::EPSILON
            || (self.width_px - width_px).abs() > f32::EPSILON
            || (self.height_px - height_px).abs() > f32::EPSILON
            || (self.opacity - opacity).abs() > f32::EPSILON
            || (self.zoom - zoom).abs() > f32::EPSILON
            || (self.node_radius_px - node_radius_px).abs() > f32::EPSILON;

        self.active = active;
        self.visible = visible;
        self.reduce_motion = reduce_motion;
        self.theme = theme;
        self.left_px = left_px;
        self.top_px = top_px;
        self.width_px = width_px;
        self.height_px = height_px;
        self.opacity = opacity;
        self.zoom = zoom;
        self.node_radius_px = node_radius_px;

        if !visible {
            self.fade = AnimatedScalar::off();
            self.started_at = None;
        } else if reduce_motion {
            self.fade = if active {
                AnimatedScalar::on()
            } else {
                AnimatedScalar::off()
            };
            self.started_at = active.then_some(now);
        } else {
            if running && !was_running {
                self.started_at = Some(now);
                self.last_tick = now;
                self.frame_accumulator = Duration::ZERO;
            }
            self.fade.set_bool(running);
        }

        if was_running != running || visual_changed {
            cx.notify();
        }
    }

    fn is_animation_active(&self) -> bool {
        self.window_active
            && !self.reduce_motion
            && self.visible
            && (self.active || self.fade.value > 0.0)
    }

    pub(crate) fn is_active_animation(&self) -> bool {
        self.is_animation_active()
    }

    fn phase_at(&self, now: Instant) -> f32 {
        let Some(started_at) = self.started_at else {
            return 0.0;
        };
        (now.saturating_duration_since(started_at).as_secs_f32() / WORKING_SNAKE_PERIOD_SECS)
            .rem_euclid(1.0)
    }

    fn start_frame_driver_if_needed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(feature = "e2e-test-support")]
        if self.frame_loop_paused_for_e2e {
            return;
        }
        if self.frame_pending || !self.is_animation_active() {
            return;
        }
        self.last_tick = cx.background_executor().now();
        self.frame_accumulator = Duration::ZERO;
        self.schedule_next_frame(window, cx);
    }

    fn schedule_next_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.frame_pending = true;
        let generation = self.frame_generation;
        cx.on_next_frame(window, move |snake, window, cx| {
            snake.on_animation_frame(generation, window, cx);
        });
    }

    fn on_animation_frame(&mut self, generation: u64, window: &mut Window, cx: &mut Context<Self>) {
        if generation != self.frame_generation {
            return;
        }
        self.frame_pending = false;
        #[cfg(feature = "e2e-test-support")]
        if self.frame_loop_paused_for_e2e {
            return;
        }
        let outcome = self.consume_display_callback_at(cx.background_executor().now(), cx);
        if outcome.next_frame_requested {
            self.schedule_next_frame(window, cx);
        }
    }

    fn consume_display_callback_at(
        &mut self,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> WorkingSnakeFrameOutcome {
        if !self.is_animation_active() {
            return WorkingSnakeFrameOutcome {
                deferred: false,
                state_advanced: false,
                repaint_notified: false,
                next_frame_requested: false,
            };
        }
        let elapsed = now.saturating_duration_since(self.last_tick);
        self.last_tick = now;
        self.frame_accumulator = self.frame_accumulator.saturating_add(elapsed);
        if self.frame_accumulator < WORKING_SNAKE_FRAME_INTERVAL {
            return WorkingSnakeFrameOutcome {
                deferred: true,
                state_advanced: false,
                repaint_notified: false,
                next_frame_requested: true,
            };
        }

        let whole_frames =
            self.frame_accumulator.as_nanos() / WORKING_SNAKE_FRAME_INTERVAL.as_nanos();
        let consumed_nanos = whole_frames.saturating_mul(WORKING_SNAKE_FRAME_INTERVAL.as_nanos());
        let consumed = Duration::from_nanos(consumed_nanos.min(u128::from(u64::MAX)) as u64);
        self.frame_accumulator = self.frame_accumulator.saturating_sub(consumed);
        self.fade.tick(consumed, MotionTokens::default().chase_fade);
        cx.notify();
        WorkingSnakeFrameOutcome {
            deferred: false,
            state_advanced: true,
            repaint_notified: true,
            next_frame_requested: self.is_animation_active(),
        }
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn reset_display_callback_for_e2e(&mut self, now: Instant) {
        self.frame_loop_paused_for_e2e = true;
        self.frame_generation = self.frame_generation.wrapping_add(1);
        self.frame_pending = false;
        self.last_tick = now;
        self.frame_accumulator = Duration::ZERO;
        self.started_at = (self.active && self.visible).then_some(now);
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn resume_display_driver_for_e2e(&mut self, cx: &mut Context<Self>) {
        self.frame_loop_paused_for_e2e = false;
        self.frame_generation = self.frame_generation.wrapping_add(1);
        self.frame_pending = false;
        self.last_tick = cx.background_executor().now();
        self.frame_accumulator = Duration::ZERO;
        cx.notify();
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn consume_display_callback_for_e2e(
        &mut self,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> WorkingSnakeFrameOutcome {
        self.consume_display_callback_at(now, cx)
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn reset_render_count_for_e2e(&mut self) {
        self.render_count_for_e2e = 0;
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn snapshot_for_e2e(&self, now: Instant) -> WorkingSnakeSnapshot {
        WorkingSnakeSnapshot {
            active: self.active && self.visible,
            next_frame_requested: self.is_animation_active(),
            frame_pending: self.frame_pending,
            phase: self.phase_at(now),
            render_count: self.render_count_for_e2e,
        }
    }
}

impl gpui::Render for WorkingSnakeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(feature = "e2e-test-support")]
        {
            self.render_count_for_e2e += 1;
        }
        self.start_frame_driver_if_needed(window, cx);
        let now = cx.background_executor().now();
        let snake = resolve_working_snake(self.theme, self.fade.value, self.phase_at(now));
        div()
            .id(("working-snake-view", self.node_id))
            .absolute()
            .left(gpui::px(self.left_px))
            .top(gpui::px(self.top_px))
            .w(gpui::px(self.width_px))
            .h(gpui::px(self.height_px))
            .opacity(self.opacity)
            .when_some(snake, |layer, snake| {
                layer.child(render_working_snake_border(
                    self.node_id,
                    snake,
                    self.zoom,
                    self.node_radius_px,
                ))
            })
    }
}
