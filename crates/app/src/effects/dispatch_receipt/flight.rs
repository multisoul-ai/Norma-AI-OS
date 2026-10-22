use super::geometry::{helix_offset, natural_flight_progress, FlightCurve};
use super::primitives::{
    centered_bounds, oriented_ellipse_path, round_quad, smoothstep, status_style, with_alpha,
};
use super::{DispatchEffectFrame, DispatchPaintPlan};
use gpui::{point, px, quad, rgba, Path, PathBuilder, Pixels, Point};
use ui::Theme;

const TRAIL_SAMPLES: usize = 24;
const HELIX_SAMPLES: usize = 22;
const TRAIL_LENGTH: f32 = 0.19;
const CORAL: u32 = 0xff6b5eff;

pub(super) fn paint_flight(frame: DispatchEffectFrame, theme: Theme, plan: &mut DispatchPaintPlan) {
    // 想法与指令栏（语音）派发共用想法流星：白色、逐步加速、尾迹随速度拉长。
    if frame.presentation.origin.uses_idea_meteor() {
        let curve = FlightCurve::new_idea_shot(frame.origin, frame.target);
        super::meteor::paint_idea_meteor(&curve, frame.presentation.progress, plan);
        return;
    }
    let travel = natural_flight_progress(frame.presentation.progress);
    if travel <= 0.0 {
        return;
    }
    // 走到这里的只有 Agent 间交接。
    let curve = FlightCurve::new_agent_handoff(frame.origin, frame.target);
    let (color, intensity) = status_style(frame.presentation.status, theme);
    let capture = smoothstep((travel - 0.84) / 0.16);
    let visible_trail = TRAIL_LENGTH * (1.0 - capture * 0.78);
    let trail_start = (travel - visible_trail).max(0.0);

    if let Some(path) = sampled_trail_path(&curve, trail_start, travel, px(6.5)) {
        plan.paths.push((
            path,
            rgba(with_alpha(0x06131fff, alpha(0x88, intensity))).into(),
        ));
    }
    if let Some(path) = sampled_trail_path(&curve, trail_start, travel, px(2.15)) {
        plan.paths
            .push((path, rgba(with_alpha(color, alpha(0xd8, intensity))).into()));
    }
    if let Some(path) = sampled_trail_path(&curve, trail_start, travel, px(0.85)) {
        plan.paths.push((
            path,
            rgba(with_alpha(0xffffffff, alpha(0xdc, intensity))).into(),
        ));
    }

    paint_helix(&curve, trail_start, travel, color, intensity, plan);
    paint_trail_motes(&curve, trail_start, travel, color, intensity, plan);
    let launch_direction = curve.tangent_at_travel(0.0);
    paint_launch_plume(
        frame.origin,
        launch_direction,
        travel,
        color,
        intensity,
        plan,
    );

    let head = curve.point_at_travel(travel);
    let tangent = curve.tangent_at_travel(travel);
    paint_ion_core(head, tangent, color, intensity, capture, plan);
}

fn sampled_trail_path(
    curve: &FlightCurve,
    start: f32,
    end: f32,
    stroke_width: Pixels,
) -> Option<Path<Pixels>> {
    let mut builder = PathBuilder::stroke(stroke_width);
    for index in 0..=TRAIL_SAMPLES {
        let ratio = index as f32 / TRAIL_SAMPLES as f32;
        let sample = curve.point_at_travel(start + (end - start) * ratio);
        if index == 0 {
            builder.move_to(sample);
        } else {
            builder.line_to(sample);
        }
    }
    builder.build().ok()
}

fn paint_helix(
    curve: &FlightCurve,
    start: f32,
    end: f32,
    color: u32,
    intensity: f32,
    plan: &mut DispatchPaintPlan,
) {
    for (phase, strand_color) in [(0.0, color), (std::f32::consts::PI, CORAL)] {
        let mut builder = PathBuilder::stroke(px(1.15));
        for index in 0..=HELIX_SAMPLES {
            let ratio = index as f32 / HELIX_SAMPLES as f32;
            let travel = start + (end - start) * ratio;
            let center = curve.point_at_travel(travel);
            let tangent = curve.tangent_at_travel(travel);
            let normal = (-tangent.1, tangent.0);
            let offset = helix_offset(ratio, phase) * intensity;
            let sample = point(
                center.x + px(normal.0 * offset),
                center.y + px(normal.1 * offset),
            );
            if index == 0 {
                builder.move_to(sample);
            } else {
                builder.line_to(sample);
            }
        }
        if let Ok(path) = builder.build() {
            plan.paths.push((
                path,
                rgba(with_alpha(strand_color, alpha(0xb8, intensity))).into(),
            ));
        }
    }
}

fn paint_trail_motes(
    curve: &FlightCurve,
    start: f32,
    end: f32,
    color: u32,
    intensity: f32,
    plan: &mut DispatchPaintPlan,
) {
    for index in 0..5 {
        let ratio = 0.12 + index as f32 * 0.17;
        let travel = start + (end - start) * ratio;
        let center = curve.point_at_travel(travel);
        let size = 2.8 - index as f32 * 0.22;
        let mote_color = if index % 2 == 0 { color } else { CORAL };
        plan.quads.push(round_quad(
            centered_bounds(center, size, size),
            size / 2.0,
            with_alpha(mote_color, alpha(0xb0, intensity * (0.72 + ratio * 0.28))),
        ));
    }
}

fn paint_launch_plume(
    origin: Point<Pixels>,
    direction: (f32, f32),
    travel: f32,
    color: u32,
    intensity: f32,
    plan: &mut DispatchPaintPlan,
) {
    if travel >= 0.22 {
        return;
    }
    let local = (travel / 0.22).clamp(0.0, 1.0);
    let fade = (1.0 - local) * intensity;
    for (size, base_alpha) in [(30.0, 0x18), (19.0, 0x30), (9.0, 0x70)] {
        plan.quads.push(round_quad(
            centered_bounds(origin, size, size),
            size / 2.0,
            with_alpha(color, alpha(base_alpha, fade)),
        ));
    }
    let mut rays = PathBuilder::stroke(px(1.1));
    let launch_angle = direction.1.atan2(direction.0);
    for index in 0..5 {
        let angle = launch_angle + (index as f32 - 2.0) * 0.34;
        let inner = 5.0 + local * 3.0;
        let outer = inner + 13.0 * (1.0 - local);
        rays.move_to(point(
            origin.x + px(angle.cos() * inner),
            origin.y + px(angle.sin() * inner),
        ));
        rays.line_to(point(
            origin.x + px(angle.cos() * outer),
            origin.y + px(angle.sin() * outer),
        ));
    }
    if let Ok(path) = rays.build() {
        plan.paths
            .push((path, rgba(with_alpha(0xffffffff, alpha(0xd0, fade))).into()));
    }
}

fn paint_ion_core(
    center: Point<Pixels>,
    tangent: (f32, f32),
    color: u32,
    intensity: f32,
    capture: f32,
    plan: &mut DispatchPaintPlan,
) {
    let scale = 1.0 - capture * 0.24;
    for (size, base_alpha) in [(34.0, 0x12), (23.0, 0x24), (14.0, 0x48)] {
        let size = size * scale;
        plan.quads.push(round_quad(
            centered_bounds(center, size, size),
            size / 2.0,
            with_alpha(color, alpha(base_alpha, intensity)),
        ));
    }
    plan.quads.push(quad(
        centered_bounds(center, 14.0 * scale, 10.0 * scale),
        px(3.5 * scale),
        rgba(with_alpha(color, alpha(0xd8, intensity))),
        px(1.0),
        rgba(with_alpha(0xffffffff, alpha(0xe8, intensity))),
        Default::default(),
    ));
    plan.quads.push(round_quad(
        centered_bounds(center, 5.2 * scale, 5.2 * scale),
        2.6 * scale,
        with_alpha(0xffffffff, alpha(0xf8, intensity)),
    ));

    if let Some(ring) = oriented_ellipse_path(center, tangent, 11.0 * scale, 4.2, 1.0) {
        plan.paths
            .push((ring, rgba(with_alpha(color, alpha(0xd8, intensity))).into()));
    }
    if let Some(ring) = oriented_ellipse_path(center, tangent, 7.0 * scale, 8.4, 0.8) {
        plan.paths
            .push((ring, rgba(with_alpha(CORAL, alpha(0xa8, intensity))).into()));
    }
    let normal = (-tangent.1, tangent.0);
    let mut flare = PathBuilder::stroke(px(1.0));
    flare.move_to(point(
        center.x - px(normal.0 * 14.0),
        center.y - px(normal.1 * 14.0),
    ));
    flare.line_to(point(
        center.x + px(normal.0 * 14.0),
        center.y + px(normal.1 * 14.0),
    ));
    if let Ok(path) = flare.build() {
        plan.paths.push((
            path,
            rgba(with_alpha(0xffffffff, alpha(0xb8, intensity))).into(),
        ));
    }
}

fn alpha(base: u8, intensity: f32) -> u8 {
    (base as f32 * intensity.clamp(0.0, 1.0)).round() as u8
}
