use ui::Theme;

use crate::motion::util::{mix_color, with_alpha, EPSILON};

pub(crate) const WORKING_SNAKE_PERIOD_SECS: f32 = 1.2;
pub(crate) const WORKING_SNAKE_INSET_PX: f32 = 3.0;
pub(crate) const WORKING_SNAKE_STROKE_PX: f32 = 3.0;
const WORKING_SNAKE_ALPHA: u8 = 0xe6;
const WORKING_SNAKE_TAIL_FRACTION: f32 = 0.24;

pub(crate) struct WorkingVisualInput {
    pub theme: Theme,
    pub stroke_color: u32,
    pub edge_highlight_color: u32,
    pub shadow_alpha: u8,
    pub shadow_y: f32,
    pub shadow_blur: f32,
    pub shadow_spread: f32,
    pub border_is_emphasized: bool,
    pub fade: f32,
    pub phase: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorkingSnake {
    pub head_phase: f32,
    pub tail_fraction: f32,
    pub head_color: u32,
    pub tail_color: u32,
    pub glow_color: u32,
    pub inset_px: f32,
    pub stroke_px: f32,
}

pub(crate) struct WorkingVisual {
    pub stroke_color: u32,
    pub edge_highlight_color: u32,
    pub shadow_color: u32,
    pub shadow_y: f32,
    pub shadow_blur: f32,
    pub shadow_spread: f32,
    pub border_is_emphasized: bool,
    pub working_snake: Option<WorkingSnake>,
}

pub(crate) fn resolve_working_visual(input: WorkingVisualInput) -> WorkingVisual {
    let fade = input.fade.clamp(0.0, 1.0);
    let snake = resolve_working_snake(input.theme, fade, input.phase);

    WorkingVisual {
        stroke_color: input.stroke_color,
        edge_highlight_color: input.edge_highlight_color,
        shadow_color: with_alpha(input.theme.shadow_color, input.shadow_alpha),
        shadow_y: input.shadow_y,
        shadow_blur: input.shadow_blur,
        shadow_spread: input.shadow_spread,
        border_is_emphasized: input.border_is_emphasized,
        working_snake: snake,
    }
}

fn resolve_working_snake(theme: Theme, fade: f32, phase: f32) -> Option<WorkingSnake> {
    if fade <= EPSILON {
        return None;
    }

    let alpha = (WORKING_SNAKE_ALPHA as f32 * fade).round() as u8;
    let normalized_phase = phase.rem_euclid(1.0);
    let head_color = with_alpha(theme.accent_cyan, alpha);
    let tail_color = with_alpha(theme.accent_violet, alpha);
    Some(WorkingSnake {
        head_phase: normalized_phase,
        tail_fraction: WORKING_SNAKE_TAIL_FRACTION,
        head_color,
        tail_color,
        glow_color: mix_color(
            head_color,
            tail_color,
            0.5 + 0.5 * phase_wave(normalized_phase),
        ),
        inset_px: WORKING_SNAKE_INSET_PX,
        stroke_px: WORKING_SNAKE_STROKE_PX,
    })
}

fn phase_wave(phase: f32) -> f32 {
    (phase.rem_euclid(1.0) * std::f32::consts::TAU).sin()
}
