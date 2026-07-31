use crate::app::MultisoulApp;
use crate::motion::{DispatchEffectPhase, DispatchEffectPresentation, DispatchReceiptVisualStatus};
use canvas::{CanvasNodeKind, Vec2};
use gpui::{
    canvas, div, point, prelude::*, px, quad, rgba, AnyElement, Background, Bounds, Edges,
    PaintQuad, Path, PathBuilder, Pixels, Point, Size, Window,
};
use std::time::{Duration, Instant};
use ui::Theme;

#[cfg(feature = "e2e-test-support")]
use crate::motion::DispatchImpactParticle;

pub(crate) const COMMAND_DOCK_BOTTOM_PX: f32 = 22.0;
pub(crate) const COMMAND_DOCK_HEIGHT_PX: f32 = 52.0;
const FLIGHT_PATH_SAMPLES: usize = 14;

#[derive(Clone, Copy, Debug)]
struct DispatchEffectFrame {
    origin: Point<Pixels>,
    target: Point<Pixels>,
    target_bounds: Bounds<Pixels>,
    presentation: DispatchEffectPresentation,
}

struct DispatchPaintPlan {
    paths: Vec<(Path<Pixels>, Background)>,
    quads: Vec<PaintQuad>,
    effect_count: usize,
    build_elapsed: Duration,
}

pub(crate) fn command_dock_center(viewport: Size<Pixels>) -> Point<Pixels> {
    point(
        viewport.width / 2.0,
        viewport.height - px(COMMAND_DOCK_BOTTOM_PX + COMMAND_DOCK_HEIGHT_PX / 2.0),
    )
}

pub(crate) fn render(app: &MultisoulApp, window: &mut Window) -> Option<AnyElement> {
    let viewport = app.runtime.document.viewport;
    let origin = command_dock_center(window.viewport_size());
    let frames = app
        .motion
        .dispatch_receipt_presentations()
        .into_iter()
        .filter_map(|presentation| {
            let node = app.runtime.document.node(presentation.target_node_id)?;
            if !matches!(node.kind, CanvasNodeKind::Agent(_)) {
                return None;
            }
            let rect = app.motion.current_node_rect(node.id, node.bounds);
            let screen_origin = viewport.canvas_to_screen(rect.origin);
            let screen_size = Vec2::new(rect.size.x * viewport.zoom, rect.size.y * viewport.zoom);
            let target_bounds = Bounds {
                origin: point(px(screen_origin.x), px(screen_origin.y)),
                size: Size {
                    width: px(screen_size.x),
                    height: px(screen_size.y),
                },
            };
            let target = point(
                target_bounds.origin.x + target_bounds.size.width / 2.0,
                target_bounds.origin.y + target_bounds.size.height / 2.0,
            );
            if !screen_point_visible(target, window.viewport_size()) {
                return None;
            }
            Some(DispatchEffectFrame {
                origin,
                target,
                target_bounds,
                presentation,
            })
        })
        .collect::<Vec<_>>();
    #[cfg(feature = "e2e-test-support")]
    let mut frames = frames;
    #[cfg(feature = "e2e-test-support")]
    if let Some(preview) = visual_preview_frame(app, origin) {
        frames.push(preview);
    }
    if frames.is_empty() {
        return None;
    }
    let theme = app.theme;
    Some(
        div()
            .id("dispatch-receipt-effect-overlay")
            .debug_selector(|| "dispatch-receipt-effect-overlay".to_string())
            .absolute()
            .inset_0()
            .child(
                canvas(
                    move |bounds, _window, _cx| build_paint_plan(bounds, &frames, theme),
                    move |_bounds, plan, window, _cx| {
                        let path_count = plan.paths.len();
                        let quad_count = plan.quads.len();
                        let paint_started_at = Instant::now();
                        for (path, background) in plan.paths {
                            window.paint_path(path, background);
                        }
                        for quad in plan.quads {
                            window.paint_quad(quad);
                        }
                        crate::frame_diagnostics::DispatchEffectPaintReport {
                            effect_count: plan.effect_count,
                            path_count,
                            quad_count,
                            build_elapsed: plan.build_elapsed,
                            paint_elapsed: paint_started_at.elapsed(),
                        }
                        .log_if_enabled();
                    },
                )
                .size_full(),
            )
            .into_any_element(),
    )
}

#[cfg(feature = "e2e-test-support")]
fn visual_preview_frame(app: &MultisoulApp, origin: Point<Pixels>) -> Option<DispatchEffectFrame> {
    let preview = std::env::var("NORMA_VISUAL_PREVIEW").ok()?;
    let (phase, progress) = match preview.as_str() {
        "dispatch-receipt-flight" => (DispatchEffectPhase::Flight, 0.62),
        "dispatch-receipt-impact" => (DispatchEffectPhase::Impact, 0.28),
        _ => return None,
    };
    let node = app
        .runtime
        .document
        .nodes()
        .iter()
        .find(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))?;
    let viewport = app.runtime.document.viewport;
    let rect = app.motion.current_node_rect(node.id, node.bounds);
    let screen_origin = viewport.canvas_to_screen(rect.origin);
    let target_bounds = Bounds {
        origin: point(px(screen_origin.x), px(screen_origin.y)),
        size: Size {
            width: px(rect.size.x * viewport.zoom),
            height: px(rect.size.y * viewport.zoom),
        },
    };
    let target = point(
        target_bounds.origin.x + target_bounds.size.width / 2.0,
        target_bounds.origin.y + target_bounds.size.height / 2.0,
    );
    Some(DispatchEffectFrame {
        origin,
        target,
        target_bounds,
        presentation: DispatchEffectPresentation {
            target_node_id: node.id,
            status: DispatchReceiptVisualStatus::Submitted,
            phase,
            progress,
            reduce_motion: false,
            particles: visual_preview_particles(),
            particle_count: 18,
        },
    })
}

#[cfg(feature = "e2e-test-support")]
fn visual_preview_particles() -> [DispatchImpactParticle; 18] {
    let mut particles = [DispatchImpactParticle::default(); 18];
    for (index, particle) in particles.iter_mut().enumerate() {
        let angle = std::f32::consts::TAU * index as f32 / 18.0;
        *particle = DispatchImpactParticle {
            direction_x: angle.cos(),
            direction_y: angle.sin(),
            distance: 30.0 + (index % 5) as f32 * 7.0,
            size: 2.0 + (index % 3) as f32 * 0.7,
            delay: (index % 4) as f32 * 0.035,
        };
    }
    particles
}

fn build_paint_plan(
    overlay_bounds: Bounds<Pixels>,
    frames: &[DispatchEffectFrame],
    theme: Theme,
) -> DispatchPaintPlan {
    let build_started_at = Instant::now();
    let mut plan = DispatchPaintPlan {
        paths: Vec::with_capacity(frames.len()),
        quads: Vec::with_capacity(frames.len() * 24),
        effect_count: frames.len(),
        build_elapsed: Duration::ZERO,
    };
    for frame in frames {
        let frame = offset_frame(*frame, overlay_bounds.origin);
        match frame.presentation.phase {
            DispatchEffectPhase::Flight => paint_flight(frame, theme, &mut plan),
            DispatchEffectPhase::Impact => paint_impact(frame, theme, &mut plan),
        }
    }
    plan.build_elapsed = build_started_at.elapsed();
    plan
}

fn offset_frame(mut frame: DispatchEffectFrame, offset: Point<Pixels>) -> DispatchEffectFrame {
    frame.origin.x += offset.x;
    frame.origin.y += offset.y;
    frame.target.x += offset.x;
    frame.target.y += offset.y;
    frame.target_bounds.origin.x += offset.x;
    frame.target_bounds.origin.y += offset.y;
    frame
}

fn paint_flight(frame: DispatchEffectFrame, theme: Theme, plan: &mut DispatchPaintPlan) {
    let progress = ease_out_cubic(frame.presentation.progress);
    let (color, intensity) = status_style(frame.presentation.status, theme);
    if let Some(path) = flight_path(frame.origin, frame.target, progress, px(2.25)) {
        plan.paths.push((
            path,
            rgba(with_alpha(color, (0x54 as f32 * intensity) as u8)).into(),
        ));
    }

    let head = bezier_point(frame.origin, frame.target, progress);
    for index in 0..7 {
        let tail_progress = (progress - index as f32 * 0.022).max(0.0);
        let tail = bezier_point(frame.origin, frame.target, tail_progress);
        let size = 8.0 - index as f32;
        let alpha = ((0xd0_u16.saturating_sub(index as u16 * 27)) as f32 * intensity) as u8;
        plan.quads.push(round_quad(
            centered_bounds(tail, size, size),
            size / 2.0,
            with_alpha(color, alpha),
        ));
    }
    plan.quads.push(receipt_quad(head, color, intensity));
}

fn paint_impact(frame: DispatchEffectFrame, theme: Theme, plan: &mut DispatchPaintPlan) {
    let progress = frame.presentation.progress.clamp(0.0, 1.0);
    let (color, intensity) = status_style(frame.presentation.status, theme);
    let pulse_alpha = ((1.0 - progress) * 0xc8 as f32 * intensity).round() as u8;
    let expansion = 3.0 + ease_out_cubic(progress) * 12.0;
    plan.quads.push(outline_quad(
        expanded_bounds(frame.target_bounds, expansion),
        18.0 + expansion,
        with_alpha(color, pulse_alpha),
        1.5,
    ));

    if frame.presentation.reduce_motion {
        let dock_alpha = ((1.0 - progress) * 0x88 as f32 * intensity).round() as u8;
        let dock_size = 12.0 + ease_out_cubic(progress) * 10.0;
        plan.quads.push(outline_quad(
            centered_bounds(frame.origin, dock_size, dock_size),
            dock_size / 2.0,
            with_alpha(color, dock_alpha),
            1.25,
        ));
        return;
    }

    for particle in frame
        .presentation
        .particles
        .iter()
        .take(frame.presentation.particle_count)
    {
        let local = ((progress - particle.delay) / (1.0 - particle.delay)).clamp(0.0, 1.0);
        if local <= 0.0 {
            continue;
        }
        let travel = ease_out_cubic(local) * particle.distance * intensity;
        let center = point(
            frame.target.x + px(particle.direction_x * travel),
            frame.target.y + px(particle.direction_y * travel),
        );
        let alpha = ((1.0 - local) * 0xe0 as f32 * intensity).round() as u8;
        plan.quads.push(round_quad(
            centered_bounds(center, particle.size, particle.size),
            particle.size / 2.0,
            with_alpha(color, alpha),
        ));
    }
}

fn flight_path(
    origin: Point<Pixels>,
    target: Point<Pixels>,
    progress: f32,
    stroke_width: Pixels,
) -> Option<Path<Pixels>> {
    if progress <= 0.0 {
        return None;
    }
    let mut builder = PathBuilder::stroke(stroke_width);
    builder.move_to(origin);
    for index in 1..=FLIGHT_PATH_SAMPLES {
        let amount = progress * index as f32 / FLIGHT_PATH_SAMPLES as f32;
        builder.line_to(bezier_point(origin, target, amount));
    }
    builder.build().ok()
}

pub(super) fn bezier_point(
    origin: Point<Pixels>,
    target: Point<Pixels>,
    progress: f32,
) -> Point<Pixels> {
    let t = progress.clamp(0.0, 1.0);
    let inverse = 1.0 - t;
    let distance_y = (f32::from(origin.y) - f32::from(target.y)).abs();
    let lift = distance_y.mul_add(0.22, 72.0).clamp(72.0, 180.0);
    let control_a = point(origin.x, origin.y - px(lift));
    let control_b = point(target.x, target.y + px((lift * 0.28).max(36.0)));
    point(
        origin.x * inverse.powi(3)
            + control_a.x * (3.0 * inverse.powi(2) * t)
            + control_b.x * (3.0 * inverse * t.powi(2))
            + target.x * t.powi(3),
        origin.y * inverse.powi(3)
            + control_a.y * (3.0 * inverse.powi(2) * t)
            + control_b.y * (3.0 * inverse * t.powi(2))
            + target.y * t.powi(3),
    )
}

fn receipt_quad(center: Point<Pixels>, color: u32, intensity: f32) -> PaintQuad {
    quad(
        centered_bounds(center, 13.0, 9.0),
        px(3.0),
        rgba(with_alpha(color, (0xf0 as f32 * intensity) as u8)),
        px(1.0),
        rgba(with_alpha(0xffffffff, (0xd8 as f32 * intensity) as u8)),
        Default::default(),
    )
}

fn round_quad(bounds: Bounds<Pixels>, radius: f32, color: u32) -> PaintQuad {
    quad(
        bounds,
        px(radius),
        rgba(color),
        Edges::<Pixels>::default(),
        rgba(0x00000000),
        Default::default(),
    )
}

fn outline_quad(bounds: Bounds<Pixels>, radius: f32, color: u32, width: f32) -> PaintQuad {
    quad(
        bounds,
        px(radius),
        rgba(0x00000000),
        px(width),
        rgba(color),
        Default::default(),
    )
}

fn centered_bounds(center: Point<Pixels>, width: f32, height: f32) -> Bounds<Pixels> {
    Bounds {
        origin: point(center.x - px(width / 2.0), center.y - px(height / 2.0)),
        size: Size {
            width: px(width),
            height: px(height),
        },
    }
}

fn expanded_bounds(bounds: Bounds<Pixels>, amount: f32) -> Bounds<Pixels> {
    Bounds {
        origin: point(bounds.origin.x - px(amount), bounds.origin.y - px(amount)),
        size: Size {
            width: bounds.size.width + px(amount * 2.0),
            height: bounds.size.height + px(amount * 2.0),
        },
    }
}

pub(super) fn screen_point_visible(point: Point<Pixels>, viewport: Size<Pixels>) -> bool {
    point.x >= px(0.0)
        && point.y >= px(0.0)
        && point.x <= viewport.width
        && point.y <= viewport.height
}

fn status_style(status: DispatchReceiptVisualStatus, theme: Theme) -> (u32, f32) {
    let intensity = impact_radius_scale(status);
    let color = match status {
        DispatchReceiptVisualStatus::Submitted => theme.accent_cyan,
        DispatchReceiptVisualStatus::Queued => theme.accent_blue,
        DispatchReceiptVisualStatus::Untracked => theme.text_secondary,
    };
    (color, intensity)
}

pub(super) fn impact_radius_scale(status: DispatchReceiptVisualStatus) -> f32 {
    match status {
        DispatchReceiptVisualStatus::Submitted => 1.0,
        DispatchReceiptVisualStatus::Queued => 0.72,
        DispatchReceiptVisualStatus::Untracked => 0.78,
    }
}

fn ease_out_cubic(progress: f32) -> f32 {
    1.0 - (1.0 - progress.clamp(0.0, 1.0)).powi(3)
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
