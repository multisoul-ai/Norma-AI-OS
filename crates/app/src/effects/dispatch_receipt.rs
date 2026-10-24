mod flight;
mod geometry;
mod impact;
mod meteor;
mod primitives;
mod styles;

use self::flight::paint_flight;
#[cfg(test)]
pub(super) use self::geometry::{
    agent_handoff_bezier_point_at_travel, helix_offset, idea_shot_bezier_point_at_travel,
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
    /// 线宽、半径、速度等绝对尺寸的缩放；画布上是 1.0，设置页缩略图更小。
    pub(super) scale: f32,
}

impl DispatchPaintPlan {
    #[cfg(test)]
    pub(super) fn empty_for_test() -> Self {
        Self {
            paths: Vec::new(),
            quads: Vec::new(),
            effect_count: 0,
            build_elapsed: Duration::ZERO,
        }
    }
}

pub(crate) struct DispatchPaintPlan {
    pub(super) paths: Vec<(Path<Pixels>, Background)>,
    pub(super) quads: Vec<PaintQuad>,
    effect_count: usize,
    build_elapsed: Duration,
}

/// 设置页缩略图的参考宽度：原型里整张迷你画布的宽，线宽与粒子尺寸按它缩放。
const PREVIEW_REFERENCE_WIDTH: f32 = 480.0;

/// 缩略图在各款式时间轴上的定格位置（ms），选在画面最有代表性的时刻。
pub(crate) fn preview_still_ms(style: crate::motion::DispatchEffectStyle) -> f32 {
    use crate::motion::DispatchEffectStyle as S;
    match style {
        S::Meteor | S::Firefly => 760.0,
        S::Plane => 720.0,
        S::Ribbon => 500.0,
        S::Fireworks | S::Vortex => 860.0,
        S::Lightning => 200.0,
        S::Ion => 300.0,
    }
}

pub(crate) fn preview_total_ms(style: crate::motion::DispatchEffectStyle) -> f32 {
    let timing = style.timing(1.0);
    (timing.flight + timing.impact).as_secs_f32() * 1000.0
}

/// 在 `bounds` 里画一帧迷你场景（指令栏在底部中间，目标卡片在右上）。
/// 与画布上同一套绘制代码，只是几何和缩放不同。
pub(crate) fn preview_plan(
    style: crate::motion::DispatchEffectStyle,
    t_ms: f32,
    bounds: Bounds<Pixels>,
    theme: Theme,
    seed: u64,
) -> DispatchPaintPlan {
    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let card = Bounds {
        origin: point(px(w * 0.5583), px(h * 0.1259)),
        size: Size {
            width: px(w * 0.375),
            height: px(h * 0.3704),
        },
    };
    let timing = style.timing(1.0);
    let flight = timing.flight.as_secs_f32() * 1000.0;
    let impact = (timing.impact.as_secs_f32() * 1000.0).max(1.0);
    let t = t_ms.clamp(0.0, flight + impact);
    let (phase, progress) = if t < flight {
        (DispatchEffectPhase::Flight, t / flight.max(1.0))
    } else {
        (DispatchEffectPhase::Impact, (t - flight) / impact)
    };
    let frame = DispatchEffectFrame {
        origin: point(px(w * 0.5), px(h * 0.8741)),
        origin_bounds: None,
        target: point(
            card.origin.x + card.size.width / 2.0,
            card.origin.y + card.size.height / 2.0,
        ),
        target_bounds: card,
        scale: (w / PREVIEW_REFERENCE_WIDTH).clamp(0.2, 1.0),
        presentation: DispatchEffectPresentation {
            style,
            seed,
            origin: DispatchVisualOrigin::CommandDock,
            target_node_id: 0,
            status: crate::motion::DispatchReceiptVisualStatus::Submitted,
            phase,
            progress: progress.clamp(0.0, 1.0),
            particles: [crate::motion::DispatchImpactParticle::default(); 28],
            particle_count: 0,
        },
    };
    build_paint_plan(bounds, &[frame], theme)
}

pub(crate) fn paint_preview_plan(plan: DispatchPaintPlan, window: &mut Window) {
    for (path, background) in plan.paths {
        window.paint_path(path, background);
    }
    for quad in plan.quads {
        window.paint_quad(quad);
    }
}

pub(crate) fn command_dock_center(viewport: Size<Pixels>) -> Point<Pixels> {
    point(
        viewport.width / 2.0,
        viewport.height - px(COMMAND_DOCK_BOTTOM_PX + COMMAND_DOCK_HEIGHT_PX / 2.0),
    )
}

pub(crate) fn render(app: &MultisoulApp, window: &mut Window) -> Option<AnyElement> {
    render_with_canvas_offset(app, window, Point::default(), Point::default())
}

pub(crate) fn render_with_canvas_offset(
    app: &MultisoulApp,
    window: &mut Window,
    canvas_offset: Point<Pixels>,
    overlay_shift: Point<Pixels>,
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
                // 窗口像素坐标，直接作为起点（叠加层覆盖整个窗口）。
                DispatchVisualOrigin::IdeaRow { x, y } => {
                    (idea_row_origin(x, y, overlay_shift), None)
                }
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
                scale: 1.0,
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
        if styles::paint_styled(&frame, theme, &mut plan) {
            continue;
        }
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

/// 想法行的窗口坐标换算成叠加层本地坐标：叠加层的原点在窗口里偏移了 `overlay_shift`。
pub(crate) fn idea_row_origin(x: i32, y: i32, overlay_shift: Point<Pixels>) -> Point<Pixels> {
    point(
        px(x as f32) - overlay_shift.x,
        px(y as f32) - overlay_shift.y,
    )
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
    let (phase, progress, agent_handoff) = match preview.as_str() {
        "dispatch-receipt-flight" => (DispatchEffectPhase::Flight, 0.62, false),
        "dispatch-receipt-impact" => (DispatchEffectPhase::Impact, 0.28, false),
        "dispatch-agent-handoff-flight" => (DispatchEffectPhase::Flight, 0.56, true),
        _ => return None,
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
        scale: 1.0,
        presentation: DispatchEffectPresentation {
            seed: 0x5eed,
            style: if agent_handoff {
                crate::motion::DispatchEffectStyle::Ion
            } else {
                crate::motion::DispatchEffectStyle::Meteor
            },
            origin: visual_origin,
            target_node_id: node.id,
            status: DispatchReceiptVisualStatus::Submitted,
            phase,
            progress,
            particles: visual_preview_particles(),
            particle_count: MAX_PARTICLES_PER_EFFECT,
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
