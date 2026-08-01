mod flight;
mod geometry;
mod impact;
mod primitives;

use self::flight::paint_flight;
#[cfg(test)]
pub(super) use self::geometry::{
    bezier_point, bezier_point_at_travel, helix_offset, natural_flight_progress,
};
use self::impact::paint_impact;
#[cfg(test)]
pub(super) use self::primitives::impact_radius_scale;
use crate::app::MultisoulApp;
use crate::motion::{DispatchEffectPhase, DispatchEffectPresentation};
use canvas::{CanvasNodeKind, Vec2};
use gpui::{
    canvas, div, point, prelude::*, px, AnyElement, Background, Bounds, PaintQuad, Path, Pixels,
    Point, Size, Window,
};
use std::time::{Duration, Instant};
use ui::Theme;

#[cfg(feature = "e2e-test-support")]
use crate::motion::dispatch_receipt::MAX_PARTICLES_PER_EFFECT;
#[cfg(feature = "e2e-test-support")]
use crate::motion::DispatchReceiptVisualStatus;
#[cfg(feature = "e2e-test-support")]
use crate::motion::{DispatchImpactParticle, DispatchImpactParticleKind};

pub(crate) const COMMAND_DOCK_BOTTOM_PX: f32 = 22.0;
pub(crate) const COMMAND_DOCK_HEIGHT_PX: f32 = 52.0;

#[derive(Clone, Copy, Debug)]
pub(super) struct DispatchEffectFrame {
    pub(super) origin: Point<Pixels>,
    pub(super) target: Point<Pixels>,
    pub(super) target_bounds: Bounds<Pixels>,
    pub(super) presentation: DispatchEffectPresentation,
}

pub(super) struct DispatchPaintPlan {
    pub(super) paths: Vec<(Path<Pixels>, Background)>,
    pub(super) quads: Vec<PaintQuad>,
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

pub(super) fn build_paint_plan(
    overlay_bounds: Bounds<Pixels>,
    frames: &[DispatchEffectFrame],
    theme: Theme,
) -> DispatchPaintPlan {
    let build_started_at = Instant::now();
    let mut plan = DispatchPaintPlan {
        paths: Vec::with_capacity(frames.len() * 16),
        quads: Vec::with_capacity(frames.len() * 36),
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

pub(super) fn screen_point_visible(point: Point<Pixels>, viewport: Size<Pixels>) -> bool {
    point.x >= px(0.0)
        && point.y >= px(0.0)
        && point.x <= viewport.width
        && point.y <= viewport.height
}

#[cfg(feature = "e2e-test-support")]
fn visual_preview_frame(app: &MultisoulApp, origin: Point<Pixels>) -> Option<DispatchEffectFrame> {
    let preview = std::env::var("NORMA_VISUAL_PREVIEW").ok()?;
    let (requested_phase, progress) = match preview.as_str() {
        "dispatch-receipt-flight" => (DispatchEffectPhase::Flight, 0.62),
        "dispatch-receipt-impact" => (DispatchEffectPhase::Impact, 0.28),
        _ => return None,
    };
    let reduce_motion = app.motion.reduce_motion;
    let phase = if reduce_motion {
        DispatchEffectPhase::Impact
    } else {
        requested_phase
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
            reduce_motion,
            particles: visual_preview_particles(),
            particle_count: if reduce_motion {
                0
            } else {
                MAX_PARTICLES_PER_EFFECT
            },
        },
    })
}

#[cfg(feature = "e2e-test-support")]
fn visual_preview_particles() -> [DispatchImpactParticle; MAX_PARTICLES_PER_EFFECT] {
    let mut particles = [DispatchImpactParticle::default(); MAX_PARTICLES_PER_EFFECT];
    for (index, particle) in particles.iter_mut().enumerate() {
        let angle = std::f32::consts::TAU * index as f32 / MAX_PARTICLES_PER_EFFECT as f32;
        let secondary = index >= 20;
        *particle = DispatchImpactParticle {
            kind: if secondary {
                DispatchImpactParticleKind::Spark
            } else if index % 5 == 0 {
                DispatchImpactParticleKind::Mote
            } else {
                DispatchImpactParticleKind::Comet
            },
            direction_x: angle.cos(),
            direction_y: angle.sin(),
            distance: if secondary {
                68.0
            } else {
                108.0 + (index % 5) as f32 * 8.0
            },
            size: if secondary {
                1.6
            } else {
                2.4 + (index % 3) as f32 * 0.5
            },
            delay: if secondary {
                0.15
            } else {
                (index % 3) as f32 * 0.012
            },
            lifetime: if secondary { 0.66 } else { 0.92 },
            twist: (index as f32 % 5.0 - 2.0) * 3.0,
        };
    }
    particles
}
