//! Terminal cursor animation model shared by app timing and paint resolution.

use gpui::Hsla;
use std::time::Duration;

use super::paint_style::TerminalGlassStyle;

pub(crate) const TERMINAL_CURSOR_BREATH_PERIOD: Duration = Duration::from_millis(1200);

/// PTY 与任务都处于运行态时，光标才使用运行中呼吸动画。
///
/// 普通 Terminal 没有上层任务，因此调用方传入 None；Agent 卡片则传入当前
/// Agent task 的运行意图，以便任务结束后即使交互式 PTY 仍存活也立即静止。
pub(crate) fn cursor_animation_running(terminal_running: bool, task_running: Option<bool>) -> bool {
    terminal_running && task_running.unwrap_or(true)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TerminalCursorAnimation {
    pub(crate) phase: f32,
    pub(crate) focused: bool,
    pub(crate) reduce_motion: bool,
    pub(crate) running: bool,
    pub(crate) display_offset: usize,
}

impl TerminalCursorAnimation {
    #[cfg(test)]
    pub(crate) const fn static_default() -> Self {
        Self {
            phase: 0.0,
            focused: false,
            reduce_motion: true,
            running: false,
            display_offset: 0,
        }
    }

    fn active(self) -> bool {
        self.focused && self.running && !self.reduce_motion && self.display_offset == 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TerminalCursorStyle {
    pub(super) fill: Hsla,
    pub(super) stroke: Hsla,
    pub(super) highlight: Hsla,
}

pub(crate) fn advance_terminal_cursor_phase(phase: f32, delta: Duration) -> f32 {
    let period = TERMINAL_CURSOR_BREATH_PERIOD.as_secs_f32();
    if period <= 0.0 {
        return 0.0;
    }
    (phase + delta.as_secs_f32() / period).rem_euclid(1.0)
}

pub(super) fn resolve_cursor_style(
    glass_style: TerminalGlassStyle,
    animation: TerminalCursorAnimation,
) -> TerminalCursorStyle {
    let alpha = cursor_alpha(animation);
    TerminalCursorStyle {
        fill: with_alpha(glass_style.cursor_fill, alpha.fill),
        stroke: with_alpha(glass_style.cursor_stroke, alpha.stroke),
        highlight: with_alpha(glass_style.cursor_highlight, alpha.highlight),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CursorAlpha {
    fill: f32,
    stroke: f32,
    highlight: f32,
}

fn cursor_alpha(animation: TerminalCursorAnimation) -> CursorAlpha {
    if !animation.active() {
        return CursorAlpha {
            fill: 0.30,
            stroke: 0.68,
            highlight: 0.30,
        };
    }

    let envelope = cursor_breath_envelope(animation.phase);
    CursorAlpha {
        fill: lerp(0.24, 0.52, envelope),
        stroke: lerp(0.62, 0.92, envelope),
        highlight: lerp(0.18, 0.62, envelope),
    }
}

fn cursor_breath_envelope(phase: f32) -> f32 {
    let s = (phase.rem_euclid(1.0) * std::f32::consts::TAU).sin();
    ((s + 1.0) / 2.0).clamp(0.0, 1.0)
}

fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t.clamp(0.0, 1.0)
}

fn with_alpha(mut color: Hsla, alpha: f32) -> Hsla {
    color.a = alpha.clamp(0.0, 1.0);
    color
}

#[cfg(test)]
#[path = "../../../tests/unit/cards/term/cursor_tests.rs"]
mod tests;
