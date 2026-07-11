//! 单个卡片动效状态到可绘制视觉参数的映射。

use super::NodeMotion;
use crate::motion::breathing::{breathing_color_for_phase, resolve_corner_glow, BreathingState};
use crate::motion::util::{alpha, mix_color, visual_priority, with_alpha, EPSILON};
use crate::motion::visual::{ResolvedNodeVisual, VisualPriority};
use crate::motion::working::{resolve_working_visual, WorkingVisualInput};
use canvas::CanvasRect;
use std::time::Instant;
use ui::Theme;

pub(super) fn resolve(
    node: &mut NodeMotion,
    canonical_rect: CanvasRect,
    theme: Theme,
    now: Option<Instant>,
) -> ResolvedNodeVisual {
    if node.dragging.target > 0.0 {
        node.current_rect = canonical_rect;
        node.target_rect = canonical_rect;
        node.layout_from = canonical_rect;
        node.layout_active = false;
    }

    let priority = visual_priority(
        node.hover.value,
        node.pressed.value,
        node.dragging.value,
        node.selected.value,
        node.focused.value,
    );
    let focus = node.focused.value;
    let selected = node.selected.value;
    let dragging = node.dragging.value;
    let pressed = node.pressed.value;
    let hover = node.hover.value;
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
    let stroke_color = if node.breathing_fade.value > EPSILON
        && node.breathing_display_state != BreathingState::Inactive
    {
        let breathing =
            breathing_color_for_phase(node.breathing_phase, node.breathing_display_state, theme);
        mix_color(base_stroke, breathing, node.breathing_fade.value)
    } else {
        base_stroke
    };
    let corner_glow = resolve_corner_glow(
        node.breathing_phase,
        node.breathing_fade.value,
        node.breathing_display_state,
    );
    let settle = node.completion_settle.value.clamp(0.0, 1.0);
    let settle_color = with_alpha(
        theme.accent_green,
        (0x54 as f32 + settle * 0xab as f32) as u8,
    );
    let stroke_color = mix_color(stroke_color, settle_color, settle * 0.88);
    let done_mix = (node.breathing_intent == crate::motion::breathing::BreathingIntent::Done) as u8
        as f32
        * 0.86;
    let completion_mix = done_mix.max(settle * 0.94);
    let edge_highlight_color = mix_color(
        with_alpha(theme.edge_highlight, edge_alpha),
        with_alpha(theme.accent_green, 0x8a),
        settle * 0.72,
    );
    let base_shadow_alpha = shadow_alpha.saturating_add((24.0 * settle) as u8);
    let base_shadow_y = (theme.shadow_y + lift * 6.0 - press_sink * 5.0 - settle * 4.0).max(8.0);
    let base_shadow_blur =
        (theme.shadow_blur + lift * 18.0 - press_sink * 6.0 + settle * 10.0).max(20.0);
    let base_shadow_spread = theme.shadow_spread - lift * 5.0 + press_sink * 2.0 - settle * 3.0;
    let working_visual = resolve_working_visual(WorkingVisualInput {
        theme,
        stroke_color,
        edge_highlight_color,
        shadow_alpha: base_shadow_alpha,
        shadow_y: base_shadow_y,
        shadow_blur: base_shadow_blur,
        shadow_spread: base_shadow_spread,
        border_is_emphasized: focus > 0.25 || selected > 0.35 || settle > 0.45,
        fade: node.chase_fade.value,
        phase: node.chase_phase_at(now),
    });

    ResolvedNodeVisual {
        priority,
        rect: node.current_rect,
        fill_color: with_alpha(theme.node_fill, fill_alpha),
        stroke_color: working_visual.stroke_color,
        status_color: mix_color(
            mix_color(theme.text_tertiary, accent_color, accent_mix),
            theme.accent_green,
            completion_mix,
        ),
        title_bar_color: with_alpha(
            theme.title_bar,
            title_alpha.saturating_add((10.0 * settle) as u8),
        ),
        edge_highlight_color: working_visual.edge_highlight_color,
        shadow_color: working_visual.shadow_color,
        shadow_y: working_visual.shadow_y,
        shadow_blur: working_visual.shadow_blur,
        shadow_spread: working_visual.shadow_spread,
        border_is_emphasized: working_visual.border_is_emphasized,
        opacity: node.opacity.value,
        corner_glow,
        working_snake: working_visual.working_snake,
    }
}
