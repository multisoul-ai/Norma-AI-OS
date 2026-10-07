mod flight;
mod geometry;
mod impact;
mod primitives;

use self::flight::paint_flight;
#[cfg(test)]
pub(super) use self::geometry::{
    agent_handoff_bezier_point_at_travel, bezier_point, bezier_point_at_travel, helix_offset,
    natural_flight_progress,
};
use self::impact::paint_impact;
#[cfg(test)]
pub(super) use self::primitives::impact_radius_scale;
use crate::app::MultisoulApp;
use crate::motion::{DispatchEffectPhase, DispatchEffectPresentation, DispatchVisualOrigin};
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
    pub(super) origin_bounds: Option<Bounds<Pixels>>,
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
    render_with_canvas_offset(app, window, Point::default())
}

pub(crate) fn render_with_canvas_offset(
    app: &MultisoulApp,
    window: &mut Window,
    canvas_offset: Point<Pixels>,
) -> Option<AnyElement> {
    let viewport = app.runtime.document.viewport;
    let dock_origin = command_dock_center(window.viewport_size());
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
                origin: point(px(screen_origin.x), px(screen_origin.y)) + canvas_offset,
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
            let (origin, origin_bounds) = match presentation.origin {
                DispatchVisualOrigin::CommandDock => (dock_origin, None),
                DispatchVisualOrigin::AgentNode(source_node_id) => {
                    let source_node = app.runtime.document.node(source_node_id)?;
                    if !matches!(source_node.kind, CanvasNodeKind::Agent(_)) {
                        return None;
                    }
                    let mut source_bounds =
                        node_screen_bounds(app, source_node.id, source_node.bounds);
                    source_bounds.origin += canvas_offset;
                    let source_center = bounds_center(source_bounds);
                    if !screen_point_visible(source_center, window.viewport_size()) {
                        return None;
                    }
                    (
                        agent_card_launch_origin(source_bounds, target),
                        Some(source_bounds),
                    )
                }
            };
            Some(DispatchEffectFrame {
                origin,
                origin_bounds,
                target,
                target_bounds,
                presentation,
            })
        })
        .collect::<Vec<_>>();
    #[cfg(feature = "e2e-test-support")]
    let mut frames = frames;
    #[cfg(feature = "e2e-test-support")]
    if let Some(preview) = visual_preview_frame(app, dock_origin) {
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
    if let Some(origin_bounds) = &mut frame.origin_bounds {
        origin_bounds.origin.x += offset.x;
        origin_bounds.origin.y += offset.y;
    }
    frame.target.x += offset.x;
    frame.target.y += offset.y;
    frame.target_bounds.origin.x += offset.x;
    frame.target_bounds.origin.y += offset.y;
    frame
}

fn node_screen_bounds(
    app: &MultisoulApp,
    node_id: canvas::NodeId,
    canonical_bounds: canvas::CanvasRect,
) -> Bounds<Pixels> {
    let viewport = app.runtime.document.viewport;
    let rect = app.motion.current_node_rect(node_id, canonical_bounds);
    let screen_origin = viewport.canvas_to_screen(rect.origin);
    Bounds {
        origin: point(px(screen_origin.x), px(screen_origin.y)),
        size: Size {
            width: px(rect.size.x * viewport.zoom),
            height: px(rect.size.y * viewport.zoom),
        },
    }
}

fn bounds_center(bounds: Bounds<Pixels>) -> Point<Pixels> {
    point(
        bounds.origin.x + bounds.size.width / 2.0,
        bounds.origin.y + bounds.size.height / 2.0,
    )
}

pub(super) fn agent_card_launch_origin(
    source_bounds: Bounds<Pixels>,
    target: Point<Pixels>,
) -> Point<Pixels> {
    let center = bounds_center(source_bounds);
    let dx = f32::from(target.x - center.x);
    let dy = f32::from(target.y - center.y);
    if dx.abs() < f32::EPSILON && dy.abs() < f32::EPSILON {
        return center;
    }
    let half_width = (f32::from(source_bounds.size.width) / 2.0).max(f32::EPSILON);
    let half_height = (f32::from(source_bounds.size.height) / 2.0).max(f32::EPSILON);
    let scale = 1.0 / (dx.abs() / half_width).max(dy.abs() / half_height);
    point(center.x + px(dx * scale), center.y + px(dy * scale))
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
    let (requested_phase, progress, agent_handoff) = match preview.as_str() {
        "dispatch-receipt-flight" => (DispatchEffectPhase::Flight, 0.62, false),
        "dispatch-receipt-impact" => (DispatchEffectPhase::Impact, 0.28, false),
        "dispatch-agent-handoff-flight" => (DispatchEffectPhase::Flight, 0.56, true),
        _ => return None,
    };
    let reduce_motion = app.motion.reduce_motion;
    let phase = if reduce_motion {
        DispatchEffectPhase::Impact
    } else {
        requested_phase
    };
    let mut agent_nodes = app
        .runtime
        .document
        .nodes()
        .iter()
        .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)));
    let source_node = agent_nodes.next()?;
    let node = if agent_handoff {
        agent_nodes.next()?
    } else {
        source_node
    };
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
    let (origin, origin_bounds, visual_origin) = if agent_handoff {
        let source_bounds = node_screen_bounds(app, source_node.id, source_node.bounds);
        (
            agent_card_launch_origin(source_bounds, target),
            Some(source_bounds),
            crate::motion::DispatchVisualOrigin::AgentNode(source_node.id),
        )
    } else {
        (
            origin,
            None,
            crate::motion::DispatchVisualOrigin::CommandDock,
        )
    };
    Some(DispatchEffectFrame {
        origin,
        origin_bounds,
        target,
        target_bounds,
        presentation: DispatchEffectPresentation {
            origin: visual_origin,
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
