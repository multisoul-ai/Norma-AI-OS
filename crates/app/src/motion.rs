//! App 层卡片动效状态：只影响渲染连续性，不改变 canvas/runtime 的真实状态。

use canvas::{CanvasRect, NodeId};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
use ui::Theme;

use crate::motion_breathing::{breathing_color_for_phase, BreathingState};
use crate::motion_util::{
    alpha, approx_rect, duration_progress, ease_out_cubic, lerp_rect, mix_color,
    reduce_motion_from_env_value, visual_priority, with_alpha, EPSILON,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionTokens {
    pub hover_in: Duration,
    pub hover_out: Duration,
    pub press: Duration,
    pub focus_selected: Duration,
    pub layout: Duration,
    pub reduced_fade: Duration,
    pub breathing_fade: Duration,
}

impl Default for MotionTokens {
    fn default() -> Self {
        Self {
            hover_in: Duration::from_millis(120),
            hover_out: Duration::from_millis(180),
            press: Duration::from_millis(100),
            focus_selected: Duration::from_millis(180),
            layout: Duration::from_millis(420),
            reduced_fade: Duration::from_millis(160),
            breathing_fade: Duration::from_millis(150),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VisualPriority {
    Idle,
    Hover,
    Pressed,
    Dragging,
    Selected,
    Focused,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedNodeVisual {
    pub priority: VisualPriority,
    pub rect: CanvasRect,
    pub fill_color: u32,
    pub stroke_color: u32,
    pub status_color: u32,
    pub title_bar_color: u32,
    pub edge_highlight_color: u32,
    pub shadow_color: u32,
    pub shadow_y: f32,
    pub shadow_blur: f32,
    pub shadow_spread: f32,
    pub border_is_emphasized: bool,
    pub opacity: f32,
}

#[derive(Debug, Clone)]
pub struct MotionStore {
    tokens: MotionTokens,
    reduce_motion: bool,
    nodes: BTreeMap<NodeId, NodeMotion>,
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
        }
    }

    pub fn tick(&mut self, delta: Duration) -> bool {
        let mut changed = false;
        for node in self.nodes.values_mut() {
            changed |= node.tick(delta, self.tokens, self.reduce_motion);
        }
        changed
    }

    pub fn retain_nodes(&mut self, live_nodes: impl IntoIterator<Item = NodeId>) {
        let live_nodes: BTreeSet<NodeId> = live_nodes.into_iter().collect();
        self.nodes.retain(|id, _| live_nodes.contains(id));
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

    pub fn set_breathing_state(&mut self, id: NodeId, state: BreathingState) {
        let reduce_motion = self.reduce_motion;
        if let Some(node) = self.nodes.get_mut(&id) {
            node.set_breathing_state(state, reduce_motion);
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

    pub fn resolve_node_visual(
        &mut self,
        id: NodeId,
        canonical_rect: CanvasRect,
        theme: Theme,
    ) -> ResolvedNodeVisual {
        self.ensure_node(id, canonical_rect);
        let node = self.nodes.get_mut(&id).expect("node was just ensured");
        node.resolve(canonical_rect, theme)
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
    breathing_fade: AnimatedScalar,
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
            breathing_fade: AnimatedScalar::off(),
        }
    }

    /// 切换呼吸色组：同一色组内不重置相位；切到新色组或重新激活时相位回到循环起点；
    /// 进入/退出激活态只驱动 breathing_fade 在 breathing_fade token 时长内淡入淡出。
    fn set_breathing_state(&mut self, state: BreathingState, reduce_motion: bool) {
        if state != BreathingState::Inactive {
            if self.breathing_state != state {
                self.breathing_phase = 0.0;
            }
            self.breathing_display_state = state;
        }
        self.breathing_state = state;
        if reduce_motion {
            self.breathing_fade.value = 0.0;
            self.breathing_fade.target = 0.0;
        } else {
            self.breathing_fade
                .set_bool(state != BreathingState::Inactive);
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
        changed |= self.breathing_fade.tick(delta, tokens.breathing_fade);
        if !reduce_motion && self.breathing_state != BreathingState::Inactive {
            let period = self.breathing_state.period();
            self.breathing_phase =
                (self.breathing_phase + delta.as_secs_f32() / period.as_secs_f32()).rem_euclid(1.0);
            changed = true;
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

    fn resolve(&mut self, canonical_rect: CanvasRect, theme: Theme) -> ResolvedNodeVisual {
        if self.dragging.target > 0.0 {
            self.current_rect = canonical_rect;
            self.target_rect = canonical_rect;
            self.layout_from = canonical_rect;
            self.layout_active = false;
        }

        let priority = visual_priority(
            self.hover.value,
            self.pressed.value,
            self.dragging.value,
            self.selected.value,
            self.focused.value,
        );
        let focus = self.focused.value;
        let selected = self.selected.value;
        let dragging = self.dragging.value;
        let pressed = self.pressed.value;
        let hover = self.hover.value;
        let emphasis = focus
            .max(selected * 0.88)
            .max(dragging * 0.76)
            .max(pressed * 0.42)
            .max(hover * 0.28)
            .clamp(0.0, 1.0);

        let accent_color = match priority {
            VisualPriority::Focused => theme.focused_stroke,
            VisualPriority::Selected => theme.selected_stroke,
            VisualPriority::Dragging => theme.focused_stroke,
            VisualPriority::Pressed => theme.selected_stroke,
            VisualPriority::Hover => theme.edge_highlight,
            VisualPriority::Idle => theme.node_stroke,
        };

        let accent_mix = match priority {
            VisualPriority::Focused => focus.max(selected * 0.72),
            VisualPriority::Selected => selected,
            VisualPriority::Dragging => dragging * 0.72,
            VisualPriority::Pressed => pressed * 0.54,
            VisualPriority::Hover => hover * 0.36,
            VisualPriority::Idle => 0.0,
        }
        .clamp(0.0, 1.0);

        let fill_alpha = alpha(theme.node_fill).saturating_add((22.0 * emphasis) as u8);
        let title_alpha = alpha(theme.title_bar).saturating_add((18.0 * emphasis) as u8);
        let edge_alpha = alpha(theme.edge_highlight).saturating_add((82.0 * emphasis) as u8);
        let shadow_alpha = alpha(theme.shadow_color).saturating_add((36.0 * emphasis) as u8);
        let lift = focus
            .max(selected * 0.58)
            .max(dragging)
            .max(hover * 0.28)
            .clamp(0.0, 1.0);
        let press_sink = pressed * (1.0 - dragging).clamp(0.0, 1.0);

        let base_stroke = mix_color(theme.node_stroke, accent_color, accent_mix);
        let stroke_color = if self.breathing_fade.value > EPSILON
            && self.breathing_display_state != BreathingState::Inactive
        {
            let breathing = breathing_color_for_phase(
                self.breathing_phase,
                self.breathing_display_state,
                theme,
            );
            mix_color(base_stroke, breathing, self.breathing_fade.value)
        } else {
            base_stroke
        };

        ResolvedNodeVisual {
            priority,
            rect: self.current_rect,
            fill_color: with_alpha(theme.node_fill, fill_alpha),
            stroke_color,
            status_color: mix_color(theme.text_tertiary, accent_color, accent_mix),
            title_bar_color: with_alpha(theme.title_bar, title_alpha),
            edge_highlight_color: with_alpha(theme.edge_highlight, edge_alpha),
            shadow_color: with_alpha(theme.shadow_color, shadow_alpha),
            shadow_y: (theme.shadow_y + lift * 6.0 - press_sink * 5.0).max(8.0),
            shadow_blur: (theme.shadow_blur + lift * 18.0 - press_sink * 6.0).max(20.0),
            shadow_spread: theme.shadow_spread - lift * 5.0 + press_sink * 2.0,
            border_is_emphasized: focus > 0.25 || selected > 0.35,
            opacity: self.opacity.value,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AnimatedScalar {
    value: f32,
    target: f32,
}

impl AnimatedScalar {
    fn off() -> Self {
        Self {
            value: 0.0,
            target: 0.0,
        }
    }

    fn on() -> Self {
        Self {
            value: 1.0,
            target: 1.0,
        }
    }

    fn set_bool(&mut self, active: bool) {
        self.target = if active { 1.0 } else { 0.0 };
    }

    fn tick(&mut self, delta: Duration, duration: Duration) -> bool {
        if (self.value - self.target).abs() <= EPSILON {
            self.value = self.target;
            return false;
        }

        if duration.is_zero() {
            self.value = self.target;
            return true;
        }

        let step = delta.as_secs_f32() / duration.as_secs_f32();
        if self.target > self.value {
            self.value = (self.value + step).min(self.target);
        } else {
            self.value = (self.value - step).max(self.target);
        }
        true
    }
}

#[cfg(test)]
#[path = "../tests/unit/motion_tests.rs"]
mod tests;
