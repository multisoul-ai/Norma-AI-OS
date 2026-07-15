//! App 层卡片动效状态：只影响渲染连续性，不改变 canvas/runtime 的真实状态。
pub(crate) mod breathing;
pub(crate) mod diagnostics;
pub(crate) mod initialization;
pub(crate) mod node_visual;
pub(crate) mod scalar;
#[cfg(test)]
mod tests;
pub(crate) mod tokens;
pub(crate) mod util;
pub(crate) mod visual;
pub(crate) mod working;
#[cfg(test)]
use crate::motion::breathing::breathing_color_for_phase;
use crate::motion::breathing::{BreathingIntent, BreathingState};
pub(crate) use crate::motion::diagnostics::MotionDiagnostics;
pub(crate) use crate::motion::initialization::{
    InitializationAnimation, InitializationAnimationKind, InitializationPresentation,
};
#[cfg(test)]
pub(crate) use crate::motion::initialization::{
    INITIALIZATION_CROSSFADE_DURATION, INITIALIZATION_CROSSFADE_START,
    INITIALIZATION_FRAME_INTERVAL, INITIALIZATION_INTERRUPT_DURATION,
    INITIALIZATION_TOTAL_DURATION,
};
use crate::motion::scalar::AnimatedScalar;
use crate::motion::tokens::MotionTokens;
#[cfg(test)]
use crate::motion::util::mix_color;
use crate::motion::util::{
    approx_rect, duration_progress, ease_out_cubic, lerp_rect, reduce_motion_from_env_value,
    EPSILON,
};
pub use crate::motion::visual::{ResolvedNodeVisual, VisualPriority};
use crate::motion::working::WORKING_SNAKE_PERIOD_SECS;
use canvas::{CanvasRect, NodeId};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};
use ui::Theme;
const BREATHING_REPAINT_INTERVAL: Duration = Duration::from_millis(33);
const WORKING_SNAKE_REPAINT_INTERVAL: Duration = Duration::from_millis(12);
#[derive(Debug, Clone)]
pub struct MotionStore {
    tokens: MotionTokens,
    pub(crate) reduce_motion: bool,
    nodes: BTreeMap<NodeId, NodeMotion>,
    initializations: BTreeMap<NodeId, InitializationAnimation>,
}
impl MotionStore {
    pub fn from_env() -> Self {
        Self::new(reduce_motion_from_env_value(std::env::var_os(
            "MULTISOUL_REDUCE_MOTION",
        )))
    }
    pub fn new(reduce_motion: bool) -> Self {
        Self {
            tokens: MotionTokens::default(),
            reduce_motion,
            nodes: BTreeMap::new(),
            initializations: BTreeMap::new(),
        }
    }
    pub fn tick(&mut self, delta: Duration) -> bool {
        let mut changed = false;
        for node in self.nodes.values_mut() {
            changed |= node.tick(delta, self.tokens, self.reduce_motion);
        }
        for initialization in self.initializations.values_mut() {
            changed |= initialization.tick(delta);
        }
        self.initializations
            .retain(|_, initialization| initialization.presentation().is_some());
        changed
    }
    /// 返回下一次 app 动画循环应等待的时长。
    ///
    /// 不活动时保留既有默认刷新率；活动初始化动画在常规刷新前临近 31ms 帧边界
    /// 时缩短 tick，从而准确到达 31ms / 806ms / 992ms 时间线节点。
    pub(crate) fn next_animation_tick_interval(&self, default: Duration) -> Duration {
        self.initializations
            .values()
            .filter_map(|animation| animation.next_visual_boundary_after())
            .filter(|duration| !duration.is_zero())
            .fold(default, |nearest, duration| nearest.min(duration))
    }
    pub fn retain_nodes(&mut self, live_nodes: impl IntoIterator<Item = NodeId>) {
        let live_nodes: BTreeSet<NodeId> = live_nodes.into_iter().collect();
        self.nodes.retain(|id, _| live_nodes.contains(id));
        self.initializations.retain(|id, _| live_nodes.contains(id));
    }
    pub fn ensure_node(&mut self, id: NodeId, rect: CanvasRect) {
        self.nodes
            .entry(id)
            .or_insert_with(|| NodeMotion::settled(rect));
    }
    pub fn spawn_node(&mut self, id: NodeId, rect: CanvasRect) {
        let mut node = NodeMotion::settled(rect);
        node.opacity.value = if self.reduce_motion { 1.0 } else { 0.0 };
        node.opacity.target = 1.0;
        self.nodes.insert(id, node);
    }
    pub(crate) fn start_initialization(&mut self, id: NodeId, kind: InitializationAnimationKind) {
        if self.nodes.contains_key(&id) {
            self.initializations
                .insert(id, InitializationAnimation::new(kind));
        }
    }
    pub(crate) fn initialization_presentation(
        &self,
        id: NodeId,
    ) -> Option<InitializationPresentation> {
        self.initializations
            .get(&id)
            .copied()
            .and_then(InitializationAnimation::presentation)
    }
    pub(crate) fn interrupt_initialization(&mut self, id: NodeId) -> bool {
        self.initializations
            .get_mut(&id)
            .map(InitializationAnimation::interrupt)
            .unwrap_or(false)
    }
    pub fn set_layout_target(&mut self, id: NodeId, target_rect: CanvasRect) {
        let node = self
            .nodes
            .entry(id)
            .or_insert_with(|| NodeMotion::settled(target_rect));
        node.set_layout_target(target_rect, self.reduce_motion);
    }
    pub fn set_hover(&mut self, id: NodeId, hovered: bool) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.hover.set_bool(hovered);
        }
    }
    pub fn set_pressed(&mut self, id: NodeId, pressed: bool) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.pressed.set_bool(pressed);
        }
    }
    pub fn set_dragging(&mut self, id: NodeId, dragging: bool) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.dragging.set_bool(dragging);
            if dragging {
                node.layout_active = false;
            }
        }
    }
    pub fn set_selection_focus(&mut self, id: NodeId, selected: bool, focused: bool) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.selected.set_bool(selected);
            node.focused.set_bool(focused);
        }
    }
    pub fn set_breathing_intent(&mut self, id: NodeId, intent: BreathingIntent) {
        let reduce_motion = self.reduce_motion;
        if let Some(node) = self.nodes.get_mut(&id) {
            node.set_breathing_intent(intent, reduce_motion);
        }
    }
    pub fn set_chase_intent(&mut self, id: NodeId, active: bool) {
        self.set_chase_intent_at(id, active, Instant::now());
    }
    pub fn set_chase_intent_at(&mut self, id: NodeId, active: bool, now: Instant) {
        let reduce_motion = self.reduce_motion;
        if let Some(node) = self.nodes.get_mut(&id) {
            node.set_chase_intent(active, reduce_motion, now);
        }
    }
    pub fn clear_transient(&mut self) {
        for node in self.nodes.values_mut() {
            node.pressed.set_bool(false);
            node.dragging.set_bool(false);
        }
    }
    pub fn sync_drag_rect(&mut self, id: NodeId, rect: CanvasRect) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.current_rect = rect;
            node.target_rect = rect;
            node.layout_from = rect;
            node.layout_active = false;
        }
    }
    #[cfg(test)]
    pub fn resolve_node_visual(
        &mut self,
        id: NodeId,
        canonical_rect: CanvasRect,
        theme: Theme,
    ) -> ResolvedNodeVisual {
        self.resolve_node_visual_with_time(id, canonical_rect, theme, None)
    }
    pub fn resolve_node_visual_at(
        &mut self,
        id: NodeId,
        canonical_rect: CanvasRect,
        theme: Theme,
        now: Instant,
    ) -> ResolvedNodeVisual {
        self.resolve_node_visual_with_time(id, canonical_rect, theme, Some(now))
    }
    fn resolve_node_visual_with_time(
        &mut self,
        id: NodeId,
        canonical_rect: CanvasRect,
        theme: Theme,
        now: Option<Instant>,
    ) -> ResolvedNodeVisual {
        self.ensure_node(id, canonical_rect);
        let node = self.nodes.get_mut(&id).expect("node was just ensured");
        node.resolve(canonical_rect, theme, now)
    }
}

#[derive(Debug, Clone)]
pub struct NodeMotion {
    current_rect: CanvasRect,
    target_rect: CanvasRect,
    layout_from: CanvasRect,
    layout_elapsed: Duration,
    layout_active: bool,
    hover: AnimatedScalar,
    pressed: AnimatedScalar,
    selected: AnimatedScalar,
    focused: AnimatedScalar,
    dragging: AnimatedScalar,
    opacity: AnimatedScalar,
    breathing_phase: f32,
    breathing_state: BreathingState,
    breathing_display_state: BreathingState,
    breathing_intent: BreathingIntent,
    breathing_fade: AnimatedScalar,
    breathing_repaint_elapsed: Duration,
    completion_settle: AnimatedScalar,
    chase_phase: f32,
    chase_started_at: Option<Instant>,
    chase_active: bool,
    chase_fade: AnimatedScalar,
    chase_repaint_elapsed: Duration,
}
impl NodeMotion {
    fn settled(rect: CanvasRect) -> Self {
        Self {
            current_rect: rect,
            target_rect: rect,
            layout_from: rect,
            layout_elapsed: Duration::ZERO,
            layout_active: false,
            hover: AnimatedScalar::off(),
            pressed: AnimatedScalar::off(),
            selected: AnimatedScalar::off(),
            focused: AnimatedScalar::off(),
            dragging: AnimatedScalar::off(),
            opacity: AnimatedScalar::on(),
            breathing_phase: 0.0,
            breathing_state: BreathingState::Inactive,
            breathing_display_state: BreathingState::Inactive,
            breathing_intent: BreathingIntent::Inactive,
            breathing_fade: AnimatedScalar::off(),
            breathing_repaint_elapsed: Duration::ZERO,
            completion_settle: AnimatedScalar::off(),
            chase_phase: 0.0,
            chase_started_at: None,
            chase_active: false,
            chase_fade: AnimatedScalar::off(),
            chase_repaint_elapsed: Duration::ZERO,
        }
    }
    fn set_breathing_intent(&mut self, intent: BreathingIntent, reduce_motion: bool) {
        let previous_intent = self.breathing_intent;
        self.breathing_intent = intent;
        let state = intent.breathing_state();
        if intent == BreathingIntent::Done && previous_intent.is_active() && !reduce_motion {
            self.completion_settle.value = 1.0;
            self.completion_settle.target = 0.0;
        }
        if state != BreathingState::Inactive {
            if self.breathing_state != state {
                self.breathing_phase = 0.0;
                self.breathing_repaint_elapsed = Duration::ZERO;
            }
            self.breathing_display_state = state;
        }
        self.breathing_state = state;
        if state == BreathingState::Inactive || reduce_motion {
            self.breathing_repaint_elapsed = Duration::ZERO;
        }
        if reduce_motion {
            self.breathing_fade.value = 0.0;
            self.breathing_fade.target = 0.0;
        } else {
            self.breathing_fade
                .set_bool(state != BreathingState::Inactive);
        }
    }
    fn set_chase_intent(&mut self, active: bool, reduce_motion: bool, now: Instant) {
        let was_active = self.chase_active;
        if !active {
            self.chase_phase = 0.0;
            self.chase_started_at = None;
            self.chase_repaint_elapsed = Duration::ZERO;
        } else if !was_active {
            self.chase_phase = 0.0;
            self.chase_started_at = Some(now);
        }
        self.chase_active = active;
        if reduce_motion {
            self.chase_fade.value = if active { 1.0 } else { 0.0 };
            self.chase_fade.target = self.chase_fade.value;
        } else {
            self.chase_fade.set_bool(active);
        }
    }
    fn set_layout_target(&mut self, target_rect: CanvasRect, reduce_motion: bool) {
        self.target_rect = target_rect;
        if reduce_motion {
            self.current_rect = target_rect;
            self.layout_from = target_rect;
            self.layout_elapsed = Duration::ZERO;
            self.layout_active = false;
        } else {
            self.layout_from = self.current_rect;
            self.layout_elapsed = Duration::ZERO;
            self.layout_active = !approx_rect(self.current_rect, target_rect);
        }
    }
    fn tick(&mut self, delta: Duration, tokens: MotionTokens, reduce_motion: bool) -> bool {
        let mut changed = false;

        changed |= self.hover.tick(
            delta,
            if self.hover.target > self.hover.value {
                tokens.hover_in
            } else {
                tokens.hover_out
            },
        );
        changed |= self.pressed.tick(delta, tokens.press);
        changed |= self.selected.tick(delta, tokens.focus_selected);
        changed |= self.focused.tick(delta, tokens.focus_selected);
        changed |= self.dragging.tick(delta, tokens.focus_selected);
        changed |= self.opacity.tick(
            delta,
            if reduce_motion {
                tokens.reduced_fade
            } else {
                tokens.focus_selected
            },
        );
        let breathing_fade_changed = self.breathing_fade.tick(delta, tokens.breathing_fade);
        changed |= breathing_fade_changed;
        changed |= self.completion_settle.tick(delta, tokens.completion_settle);
        let chase_fade_changed = self.chase_fade.tick(delta, tokens.chase_fade);
        changed |= chase_fade_changed;
        if !reduce_motion && self.breathing_state != BreathingState::Inactive {
            let breathing_is_fading =
                breathing_fade_changed || self.breathing_fade.value < 1.0 - EPSILON;
            let period = self.breathing_state.period();
            self.breathing_phase =
                (self.breathing_phase + delta.as_secs_f32() / period.as_secs_f32()).rem_euclid(1.0);
            self.breathing_repaint_elapsed += delta;
            if breathing_is_fading || self.breathing_repaint_elapsed >= BREATHING_REPAINT_INTERVAL {
                self.breathing_repaint_elapsed = Duration::ZERO;
                changed = true;
            }
        }
        if !reduce_motion && self.chase_active {
            self.advance_chase_phase(delta);
            if chase_fade_changed || self.chase_fade.value < 1.0 - EPSILON {
                self.chase_repaint_elapsed = Duration::ZERO;
                changed = true;
            } else {
                self.chase_repaint_elapsed += delta;
                if self.chase_repaint_elapsed >= WORKING_SNAKE_REPAINT_INTERVAL {
                    self.chase_repaint_elapsed = Duration::ZERO;
                    changed = true;
                }
            }
        }
        if self.dragging.target > 0.0 {
            self.layout_active = false;
            return changed;
        }
        if self.layout_active {
            self.layout_elapsed += delta;
            let raw = duration_progress(self.layout_elapsed, tokens.layout);
            let eased = ease_out_cubic(raw);
            self.current_rect = lerp_rect(self.layout_from, self.target_rect, eased);
            if raw >= 1.0 {
                self.current_rect = self.target_rect;
                self.layout_active = false;
            }
            changed = true;
        }

        changed
    }
    fn advance_chase_phase(&mut self, delta: Duration) {
        self.chase_phase =
            (self.chase_phase + delta.as_secs_f32() / WORKING_SNAKE_PERIOD_SECS).rem_euclid(1.0);
    }

    fn resolve(
        &mut self,
        canonical_rect: CanvasRect,
        theme: Theme,
        now: Option<Instant>,
    ) -> ResolvedNodeVisual {
        node_visual::resolve(self, canonical_rect, theme, now)
    }

    fn chase_phase_at(&self, now: Option<Instant>) -> f32 {
        let Some(now) = now else {
            return self.chase_phase;
        };
        let Some(started_at) = self.chase_started_at else {
            return self.chase_phase;
        };
        (now.saturating_duration_since(started_at).as_secs_f32() / WORKING_SNAKE_PERIOD_SECS)
            .rem_euclid(1.0)
    }
}
