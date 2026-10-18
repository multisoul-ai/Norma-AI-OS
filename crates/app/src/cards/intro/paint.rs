//! 初始化 ASCII 帧的批量自绘。
//!
//! 每帧只建立按颜色合并后的 text run，并以一个 GPUI canvas 绘制；不会为 4100
//! 个字符分别创建 div。画布始终等比居中，标题栏由外层 card chrome 单独渲染。

use super::{intro_frame, IntroFrame, INTRO_ANIMATION_COLUMNS, INTRO_ANIMATION_ROWS};
use crate::motion::{InitializationAnimationKind, InitializationPresentation};
use gpui::{
    canvas, div, point, prelude::*, px, quad, rgba, transparent_black, AnyElement, App, Bounds,
    Edges, Font, FontFeatures, FontStyle, FontWeight, Hsla, Pixels, Point, SharedString, TextRun,
    Window,
};
use ui::Theme;

const BASE_CELL_WIDTH: f32 = 8.0;
const BASE_LINE_HEIGHT: f32 = 16.0;
const BASE_FONT_SIZE: f32 = 13.0;
const GHOSTTY_ACCENT: u32 = 0x3551f3ff;
const CODEX_ACCENT: u32 = 0x7f94ffff;
const CLAUDE_CODE_ACCENT: u32 = 0xdf7858ff;
const CURSOR_ACCENT: u32 = 0x9b8cff;
const NEUTRAL_GLYPH: u32 = 0xc3c3c4ff;
const INTRO_BACKGROUND: u32 = 0x0f0f11ff;

#[derive(Clone, Copy, Debug, PartialEq)]
struct IntroMetrics {
    font_size: Pixels,
    cell_width: Pixels,
    line_height: Pixels,
    origin: Point<Pixels>,
}

#[derive(Clone, Debug, PartialEq)]
struct IntroTextRun {
    text: String,
    row: usize,
    start_column: usize,
    accent: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct IntroLayout {
    metrics: IntroMetrics,
    text_runs: Vec<IntroTextRun>,
}

pub(crate) fn render_intro_overlay(
    presentation: InitializationPresentation,
    theme: Theme,
    bottom_radius_px: f32,
) -> AnyElement {
    let frame = intro_frame(presentation.kind, presentation.frame_index);
    div()
        .absolute()
        .inset_0()
        .overflow_hidden()
        .rounded_b(px(bottom_radius_px))
        .opacity(presentation.overlay_opacity)
        .child(render_intro_canvas(frame, presentation.kind, theme))
        .into_any_element()
}

fn render_intro_canvas(
    frame: IntroFrame,
    kind: InitializationAnimationKind,
    theme: Theme,
) -> impl IntoElement {
    canvas(
        move |bounds, _window, _cx| build_intro_layout(frame, bounds),
        move |bounds, layout, window, cx| {
            paint_intro_layout(bounds, &layout, kind, theme, window, cx)
        },
    )
    .size_full()
}

fn build_intro_layout(frame: IntroFrame, bounds: Bounds<Pixels>) -> IntroLayout {
    let metrics = centered_metrics(bounds);
    let mut text_runs = Vec::new();
    for row in 0..INTRO_ANIMATION_ROWS {
        let mut active: Option<IntroTextRun> = None;
        for column in 0..INTRO_ANIMATION_COLUMNS {
            let cell = frame.cell(row, column);
            if cell.glyph == ' ' {
                flush_run(&mut text_runs, &mut active);
                continue;
            }
            match active.as_mut() {
                Some(run) if run.accent == cell.accent => run.text.push(cell.glyph),
                Some(_) => {
                    flush_run(&mut text_runs, &mut active);
                    active = Some(IntroTextRun {
                        text: cell.glyph.to_string(),
                        row,
                        start_column: column,
                        accent: cell.accent,
                    });
                }
                None => {
                    active = Some(IntroTextRun {
                        text: cell.glyph.to_string(),
                        row,
                        start_column: column,
                        accent: cell.accent,
                    });
                }
            }
        }
        flush_run(&mut text_runs, &mut active);
    }
    IntroLayout { metrics, text_runs }
}

fn centered_metrics(bounds: Bounds<Pixels>) -> IntroMetrics {
    let base_width = BASE_CELL_WIDTH * INTRO_ANIMATION_COLUMNS as f32;
    let base_height = BASE_LINE_HEIGHT * INTRO_ANIMATION_ROWS as f32;
    let scale = (f32::from(bounds.size.width) / base_width)
        .min(f32::from(bounds.size.height) / base_height)
        .max(0.0);
    let art_width = px(base_width * scale);
    let art_height = px(base_height * scale);
    IntroMetrics {
        font_size: px(BASE_FONT_SIZE * scale),
        cell_width: px(BASE_CELL_WIDTH * scale),
        line_height: px(BASE_LINE_HEIGHT * scale),
        origin: point(
            bounds.origin.x + (bounds.size.width - art_width) / 2.0,
            bounds.origin.y + (bounds.size.height - art_height) / 2.0,
        ),
    }
}

fn flush_run(runs: &mut Vec<IntroTextRun>, active: &mut Option<IntroTextRun>) {
    if let Some(run) = active.take() {
        runs.push(run);
    }
}

fn paint_intro_layout(
    bounds: Bounds<Pixels>,
    layout: &IntroLayout,
    kind: InitializationAnimationKind,
    theme: Theme,
    window: &mut Window,
    cx: &mut App,
) {
    window.paint_quad(quad(
        bounds,
        px(0.0),
        rgba(INTRO_BACKGROUND),
        Edges::<Pixels>::default(),
        transparent_black(),
        Default::default(),
    ));
    for run in &layout.text_runs {
        let text: SharedString = run.text.clone().into();
        let shaped = window.text_system().shape_line(
            text,
            layout.metrics.font_size,
            &[TextRun {
                len: run.text.len(),
                font: Font {
                    family: "Menlo".into(),
                    features: FontFeatures::default(),
                    fallbacks: None,
                    weight: FontWeight::NORMAL,
                    style: FontStyle::Normal,
                },
                color: intro_text_color(kind, run.accent, theme),
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        );
        let origin = point(
            layout.metrics.origin.x + layout.metrics.cell_width * run.start_column as f32,
            layout.metrics.origin.y
                + layout.metrics.line_height * run.row as f32
                + vertical_text_offset(layout.metrics),
        );
        let _ = shaped.paint(origin, layout.metrics.line_height, window, cx);
    }
}

fn intro_text_color(kind: InitializationAnimationKind, accent: bool, theme: Theme) -> Hsla {
    if !accent {
        return rgba(NEUTRAL_GLYPH).into();
    }
    match kind {
        InitializationAnimationKind::Terminal => rgba(GHOSTTY_ACCENT).into(),
        InitializationAnimationKind::Codex => rgba(CODEX_ACCENT).into(),
        InitializationAnimationKind::Claude => rgba(CLAUDE_CODE_ACCENT).into(),
        InitializationAnimationKind::Cursor => rgba(CURSOR_ACCENT).into(),
        InitializationAnimationKind::Grok => rgba(theme.accent_violet).into(),
        InitializationAnimationKind::OpenCode => rgba(theme.accent_violet).into(),
        InitializationAnimationKind::Kimi => rgba(theme.accent_violet).into(),
        InitializationAnimationKind::ZCode => rgba(theme.accent_violet).into(),
        InitializationAnimationKind::Pi => rgba(theme.accent_violet).into(),
        InitializationAnimationKind::Hermes => rgba(theme.accent_violet).into(),
    }
}

fn vertical_text_offset(metrics: IntroMetrics) -> Pixels {
    px(((f32::from(metrics.line_height) - f32::from(metrics.font_size)) / 2.0).max(0.0))
}

#[cfg(test)]
#[path = "../../../tests/unit/intro_animation_asset_tests.rs"]
mod tests;
