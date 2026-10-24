use super::primitives::{
    centered_bounds, ease_out_cubic, expanded_bounds, oriented_ellipse_path, outline_quad,
    round_quad, smoothstep, status_style, with_alpha,
};
use super::{DispatchEffectFrame, DispatchPaintPlan};
use crate::motion::{DispatchImpactParticle, DispatchImpactParticleKind};
use gpui::{point, px, rgba, PathBuilder, Pixels, Point};
use ui::Theme;

const CORAL: u32 = 0xff6b5eff;
const GOLD: u32 = 0xffc857ff;

pub(super) fn paint_impact(frame: DispatchEffectFrame, theme: Theme, plan: &mut DispatchPaintPlan) {
    let progress = frame.presentation.progress.clamp(0.0, 1.0);
    let (status_color, intensity) = status_style(frame.presentation.status, theme);
    // 想法流星撞击是白色的，与飞行时的白色光点衔接。
    let style = frame.presentation.style;
    let idea_meteor = style == crate::motion::DispatchEffectStyle::Meteor;
    let color = if idea_meteor {
        0xeaf6ffff
    } else {
        status_color
    };
    if idea_meteor {
        let curve = super::geometry::FlightCurve::new_idea_shot(frame.origin, frame.target);
        super::meteor::paint_idea_afterglow(&curve, progress, plan);
    }
    let fade = 1.0 - smoothstep(progress);
    let expansion = 3.0 + ease_out_cubic(progress) * 18.0;
    plan.quads.push(outline_quad(
        expanded_bounds(frame.target_bounds, expansion),
        18.0 + expansion,
        with_alpha(color, alpha(0xc8, fade * intensity)),
        1.5,
    ));

    paint_central_flash(frame.target, color, intensity, progress, plan);
    paint_shockwave(frame.target, color, intensity, progress, plan);
    paint_star_rays(frame.target, color, intensity, progress, plan);
    paint_arc_fragments(frame.target, color, intensity, progress, plan);
    paint_horizontal_flare(frame.target, color, intensity, progress, plan);

    paint_particles(
        frame.target,
        &frame.presentation.particles[..frame.presentation.particle_count],
        color,
        intensity,
        progress,
        plan,
    );
}

fn paint_central_flash(
    center: Point<Pixels>,
    color: u32,
    intensity: f32,
    progress: f32,
    plan: &mut DispatchPaintPlan,
) {
    let expansion = ease_out_cubic(progress);
    let fade = (1.0 - smoothstep(progress)) * intensity;
    for (base, growth, base_alpha) in [(16.0, 54.0, 0x20), (10.0, 34.0, 0x40), (5.0, 18.0, 0xa8)] {
        let size = base + growth * expansion;
        plan.quads.push(round_quad(
            centered_bounds(center, size, size),
            size / 2.0,
            with_alpha(color, alpha(base_alpha, fade)),
        ));
    }
    let core_size = 7.0 + 5.0 * (1.0 - progress);
    plan.quads.push(round_quad(
        centered_bounds(center, core_size, core_size),
        core_size / 2.0,
        with_alpha(0xffffffff, alpha(0xf0, fade)),
    ));
}

fn paint_shockwave(
    center: Point<Pixels>,
    color: u32,
    intensity: f32,
    progress: f32,
    plan: &mut DispatchPaintPlan,
) {
    let radius = 8.0 + ease_out_cubic(progress) * 92.0 * intensity;
    if let Some(path) = oriented_ellipse_path(center, (1.0, 0.0), radius, radius, 1.5) {
        plan.paths.push((
            path,
            rgba(with_alpha(
                color,
                alpha(0xb8, (1.0 - smoothstep(progress)) * intensity),
            ))
            .into(),
        ));
    }
}

fn paint_star_rays(
    center: Point<Pixels>,
    color: u32,
    intensity: f32,
    progress: f32,
    plan: &mut DispatchPaintPlan,
) {
    if progress >= 0.34 {
        return;
    }
    let local = progress / 0.34;
    let mut builder = PathBuilder::stroke(px(1.35));
    for index in 0..8 {
        let angle = std::f32::consts::TAU * index as f32 / 8.0 + 0.18;
        let inner = 7.0 + local * 12.0;
        let outer = 24.0 + ease_out_cubic(local) * 48.0;
        builder.move_to(point(
            center.x + px(angle.cos() * inner),
            center.y + px(angle.sin() * inner),
        ));
        builder.line_to(point(
            center.x + px(angle.cos() * outer),
            center.y + px(angle.sin() * outer),
        ));
    }
    if let Ok(path) = builder.build() {
        plan.paths.push((
            path,
            rgba(with_alpha(color, alpha(0xe0, (1.0 - local) * intensity))).into(),
        ));
    }
}

fn paint_arc_fragments(
    center: Point<Pixels>,
    color: u32,
    intensity: f32,
    progress: f32,
    plan: &mut DispatchPaintPlan,
) {
    let radius = 15.0 + ease_out_cubic(progress) * 58.0 * intensity;
    for fragment in 0..3 {
        let rotation = progress * 2.4 + fragment as f32 * std::f32::consts::TAU / 3.0;
        let mut builder = PathBuilder::stroke(px(1.1));
        for index in 0..=8 {
            let angle = rotation + index as f32 / 8.0 * 0.72;
            let sample = point(
                center.x + px(angle.cos() * radius),
                center.y + px(angle.sin() * radius),
            );
            if index == 0 {
                builder.move_to(sample);
            } else {
                builder.line_to(sample);
            }
        }
        if let Ok(path) = builder.build() {
            let arc_color = if fragment == 1 { CORAL } else { color };
            plan.paths.push((
                path,
                rgba(with_alpha(
                    arc_color,
                    alpha(0xa8, (1.0 - progress) * intensity),
                ))
                .into(),
            ));
        }
    }
}

fn paint_horizontal_flare(
    center: Point<Pixels>,
    color: u32,
    intensity: f32,
    progress: f32,
    plan: &mut DispatchPaintPlan,
) {
    if progress >= 0.46 {
        return;
    }
    let local = progress / 0.46;
    let half_width = 18.0 + ease_out_cubic(local) * 72.0;
    let mut builder = PathBuilder::stroke(px(1.0));
    builder.move_to(point(center.x - px(half_width), center.y));
    builder.line_to(point(center.x + px(half_width), center.y));
    if let Ok(path) = builder.build() {
        plan.paths.push((
            path,
            rgba(with_alpha(
                0xffffffff,
                alpha(0xc8, (1.0 - local) * intensity),
            ))
            .into(),
        ));
    }
    plan.quads.push(round_quad(
        centered_bounds(center, half_width * 0.9, 2.0),
        1.0,
        with_alpha(color, alpha(0x48, (1.0 - local) * intensity)),
    ));
}

fn paint_particles(
    origin: Point<Pixels>,
    particles: &[DispatchImpactParticle],
    color: u32,
    intensity: f32,
    progress: f32,
    plan: &mut DispatchPaintPlan,
) {
    let mut comet_builders = [
        PathBuilder::stroke(px(2.1)),
        PathBuilder::stroke(px(2.55)),
        PathBuilder::stroke(px(3.0)),
        PathBuilder::stroke(px(3.45)),
    ];
    let mut comet_counts = [0_usize; 4];
    let mut comet_fades = [0.0_f32; 4];
    let mut spark_builder = PathBuilder::stroke(px(0.9));
    let mut spark_count = 0_usize;
    let mut spark_fade = 0.0_f32;

    for particle in particles {
        let Some(state) = particle_state(origin, *particle, intensity, progress) else {
            continue;
        };
        match particle.kind {
            DispatchImpactParticleKind::Comet => {
                let tail_length = 8.0 + (1.0 - state.local) * 13.0;
                let tail = point(
                    state.center.x - px(particle.direction_x * tail_length),
                    state.center.y - px(particle.direction_y * tail_length),
                );
                let group = (((particle.size - 2.1) / 0.45).round() as usize).min(3);
                comet_builders[group].move_to(tail);
                comet_builders[group].line_to(state.center);
                comet_counts[group] += 1;
                comet_fades[group] = comet_fades[group].max(state.fade);
                plan.quads.push(round_quad(
                    centered_bounds(state.center, particle.size * 1.25, particle.size * 1.25),
                    particle.size,
                    with_alpha(0xffffffff, alpha(0xe8, state.fade)),
                ));
            }
            DispatchImpactParticleKind::Mote => {
                plan.quads.push(round_quad(
                    centered_bounds(state.center, particle.size * 3.2, particle.size * 3.2),
                    particle.size * 1.6,
                    with_alpha(CORAL, alpha(0x30, state.fade)),
                ));
                plan.quads.push(round_quad(
                    centered_bounds(state.center, particle.size * 1.3, particle.size * 1.3),
                    particle.size,
                    with_alpha(0xffffffff, alpha(0xd8, state.fade)),
                ));
            }
            DispatchImpactParticleKind::Spark => {
                let half = 2.5 + particle.size;
                spark_builder.move_to(point(state.center.x - px(half), state.center.y));
                spark_builder.line_to(point(state.center.x + px(half), state.center.y));
                spark_builder.move_to(point(state.center.x, state.center.y - px(half)));
                spark_builder.line_to(point(state.center.x, state.center.y + px(half)));
                spark_count += 1;
                spark_fade = spark_fade.max(state.fade);
            }
        }
    }

    for (index, builder) in comet_builders.into_iter().enumerate() {
        if comet_counts[index] == 0 {
            continue;
        }
        if let Ok(path) = builder.build() {
            plan.paths.push((
                path,
                rgba(with_alpha(color, alpha(0xd8, comet_fades[index]))).into(),
            ));
        }
    }
    if spark_count > 0 {
        if let Ok(path) = spark_builder.build() {
            plan.paths
                .push((path, rgba(with_alpha(GOLD, alpha(0xe8, spark_fade))).into()));
        }
    }
}

struct ParticleState {
    center: Point<Pixels>,
    local: f32,
    fade: f32,
}

fn particle_state(
    origin: Point<Pixels>,
    particle: DispatchImpactParticle,
    intensity: f32,
    progress: f32,
) -> Option<ParticleState> {
    let local = (progress - particle.delay) / particle.lifetime.max(f32::EPSILON);
    if !(0.0..=1.0).contains(&local) {
        return None;
    }
    let drag = (1.0 - (-2.4 * local).exp()) / (1.0 - (-2.4_f32).exp());
    let normal = (-particle.direction_y, particle.direction_x);
    let twist = (std::f32::consts::PI * local).sin() * particle.twist;
    let distance = particle.distance * drag * intensity;
    Some(ParticleState {
        center: point(
            origin.x + px(particle.direction_x * distance + normal.0 * twist),
            origin.y + px(particle.direction_y * distance + normal.1 * twist + 8.0 * local.powi(2)),
        ),
        local,
        fade: (1.0 - smoothstep(local)) * intensity,
    })
}

fn alpha(base: u8, intensity: f32) -> u8 {
    (base as f32 * intensity.clamp(0.0, 1.0)).round() as u8
}
