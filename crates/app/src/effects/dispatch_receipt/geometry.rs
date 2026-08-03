use gpui::{point, px, Pixels, Point};

const ARC_LENGTH_SAMPLES: usize = 48;
const HELIX_COILS: f32 = 13.0;
const HELIX_RADIUS: f32 = 4.6;

#[derive(Clone, Copy)]
struct CurveSample {
    parameter: f32,
    cumulative_length: f32,
}

pub(super) struct FlightCurve {
    origin: Point<Pixels>,
    control_a: Point<Pixels>,
    control_b: Point<Pixels>,
    target: Point<Pixels>,
    samples: [CurveSample; ARC_LENGTH_SAMPLES + 1],
    total_length: f32,
}

impl FlightCurve {
    pub(super) fn new(origin: Point<Pixels>, target: Point<Pixels>) -> Self {
        let (control_a, control_b) = control_points(origin, target);
        Self::from_control_points(origin, control_a, control_b, target)
    }

    pub(super) fn new_agent_handoff(origin: Point<Pixels>, target: Point<Pixels>) -> Self {
        let (control_a, control_b) = agent_handoff_control_points(origin, target);
        Self::from_control_points(origin, control_a, control_b, target)
    }

    fn from_control_points(
        origin: Point<Pixels>,
        control_a: Point<Pixels>,
        control_b: Point<Pixels>,
        target: Point<Pixels>,
    ) -> Self {
        let mut samples = [CurveSample {
            parameter: 0.0,
            cumulative_length: 0.0,
        }; ARC_LENGTH_SAMPLES + 1];
        let mut previous = origin;
        let mut total_length = 0.0;
        for index in 1..=ARC_LENGTH_SAMPLES {
            let parameter = index as f32 / ARC_LENGTH_SAMPLES as f32;
            let current = cubic_point(origin, control_a, control_b, target, parameter);
            total_length += distance(previous, current);
            samples[index] = CurveSample {
                parameter,
                cumulative_length: total_length,
            };
            previous = current;
        }
        Self {
            origin,
            control_a,
            control_b,
            target,
            samples,
            total_length,
        }
    }

    pub(super) fn point_at_travel(&self, travel: f32) -> Point<Pixels> {
        let travel = travel.clamp(0.0, 1.0);
        if travel <= 0.0 {
            return self.origin;
        }
        if travel >= 1.0 {
            return self.target;
        }
        let wanted_length = self.total_length * travel;
        let upper_index = self
            .samples
            .partition_point(|sample| sample.cumulative_length < wanted_length)
            .min(self.samples.len() - 1);
        let lower = self.samples[upper_index - 1];
        let upper = self.samples[upper_index];
        let span = (upper.cumulative_length - lower.cumulative_length).max(f32::EPSILON);
        let mix = (wanted_length - lower.cumulative_length) / span;
        let parameter = lower.parameter + (upper.parameter - lower.parameter) * mix;
        cubic_point(
            self.origin,
            self.control_a,
            self.control_b,
            self.target,
            parameter,
        )
    }

    pub(super) fn tangent_at_travel(&self, travel: f32) -> (f32, f32) {
        let before = self.point_at_travel((travel - 0.002).max(0.0));
        let after = self.point_at_travel((travel + 0.002).min(1.0));
        let dx = f32::from(after.x - before.x);
        let dy = f32::from(after.y - before.y);
        let length = dx.hypot(dy).max(f32::EPSILON);
        (dx / length, dy / length)
    }
}

#[cfg(test)]
pub(crate) fn bezier_point(
    origin: Point<Pixels>,
    target: Point<Pixels>,
    progress: f32,
) -> Point<Pixels> {
    let (control_a, control_b) = control_points(origin, target);
    cubic_point(origin, control_a, control_b, target, progress)
}

#[cfg(test)]
pub(crate) fn bezier_point_at_travel(
    origin: Point<Pixels>,
    target: Point<Pixels>,
    travel: f32,
) -> Point<Pixels> {
    FlightCurve::new(origin, target).point_at_travel(travel)
}

#[cfg(test)]
pub(crate) fn agent_handoff_bezier_point_at_travel(
    origin: Point<Pixels>,
    target: Point<Pixels>,
    travel: f32,
) -> Point<Pixels> {
    FlightCurve::new_agent_handoff(origin, target).point_at_travel(travel)
}

pub(crate) fn natural_flight_progress(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    if progress <= 0.0 || progress >= 1.0 {
        return progress;
    }
    (progress + (std::f32::consts::PI * progress).sin() * 0.09).clamp(0.0, 1.0)
}

pub(crate) fn helix_offset(ratio: f32, phase: f32) -> f32 {
    let ratio = ratio.clamp(0.0, 1.0);
    if ratio <= 0.0 || ratio >= 1.0 {
        return 0.0;
    }
    let envelope = (std::f32::consts::PI * ratio).sin();
    (ratio * std::f32::consts::PI * HELIX_COILS + phase).sin() * envelope * HELIX_RADIUS
}

fn control_points(origin: Point<Pixels>, target: Point<Pixels>) -> (Point<Pixels>, Point<Pixels>) {
    let horizontal = f32::from(target.x - origin.x);
    let vertical = f32::from(origin.y - target.y).max(0.0);
    let direct_distance = horizontal.hypot(f32::from(target.y - origin.y));
    let lift = (direct_distance * 0.34).clamp(150.0, 252.0);
    let settle = (vertical * 0.17).clamp(68.0, 132.0);
    (
        point(origin.x + px(horizontal * 0.045), origin.y - px(lift)),
        point(target.x - px(horizontal * 0.18), target.y + px(settle)),
    )
}

fn agent_handoff_control_points(
    origin: Point<Pixels>,
    target: Point<Pixels>,
) -> (Point<Pixels>, Point<Pixels>) {
    let dx = f32::from(target.x - origin.x);
    let dy = f32::from(target.y - origin.y);
    let distance = dx.hypot(dy).max(f32::EPSILON);
    let mut normal = (-dy / distance, dx / distance);
    if normal.1 > 0.0 {
        normal = (-normal.0, -normal.1);
    }
    let bend = (distance * 0.22).clamp(36.0, 160.0);
    (
        point(
            origin.x + px(dx * 0.28 + normal.0 * bend),
            origin.y + px(dy * 0.28 + normal.1 * bend),
        ),
        point(
            origin.x + px(dx * 0.72 + normal.0 * bend),
            origin.y + px(dy * 0.72 + normal.1 * bend),
        ),
    )
}

fn cubic_point(
    origin: Point<Pixels>,
    control_a: Point<Pixels>,
    control_b: Point<Pixels>,
    target: Point<Pixels>,
    parameter: f32,
) -> Point<Pixels> {
    let parameter = parameter.clamp(0.0, 1.0);
    let inverse = 1.0 - parameter;
    point(
        origin.x * inverse.powi(3)
            + control_a.x * (3.0 * inverse.powi(2) * parameter)
            + control_b.x * (3.0 * inverse * parameter.powi(2))
            + target.x * parameter.powi(3),
        origin.y * inverse.powi(3)
            + control_a.y * (3.0 * inverse.powi(2) * parameter)
            + control_b.y * (3.0 * inverse * parameter.powi(2))
            + target.y * parameter.powi(3),
    )
}

fn distance(a: Point<Pixels>, b: Point<Pixels>) -> f32 {
    let dx = f32::from(b.x - a.x);
    let dy = f32::from(b.y - a.y);
    dx.hypot(dy)
}
