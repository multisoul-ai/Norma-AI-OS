//! 纸飞机：折叠成形 → 沿弧线滑翔并随转弯侧倾 → 落入卡片标题栏 → 确认勾。
//! 时间轴（ms）：折叠 0–220，滑翔 220–1020，落入与确认 1020–1620。

use super::painter::*;
use super::StyleFrame;

const FOLD: f32 = 220.0;
const LAND_AT: f32 = 1020.0;
const FLY: f32 = LAND_AT - FOLD;
const END: f32 = 600.0;
const DIAMOND: [(f32, f32); 4] = [(8.0, 0.0), (0.0, -8.0), (-8.0, 0.0), (0.0, 8.0)];
const PLANE: [(f32, f32); 4] = [(12.0, 0.0), (-8.0, -8.0), (-4.0, 0.0), (-8.0, 8.0)];
const DASH: f32 = 5.0;
const DASH_PERIOD: f32 = 11.0;
const TRAIL_SPAN: f32 = 0.3;

struct Course {
    path: Poly,
    end: V,
}

fn course(f: &StyleFrame) -> Course {
    let from = f.origin;
    let end = v(f.card.x + f.card.w / 2.0, f.card.y + 16.0);
    let d = from.dist(end).max(1.0);
    let sign = if end.x >= from.x { 1.0 } else { -1.0 };
    let path = Poly::sampled(90, |t| {
        cubic_bezier(
            from,
            v(from.x + sign * d * 0.08, from.y - (d * 0.42).max(90.0)),
            v(end.x - sign * d * 0.32, end.y - 80.0),
            end,
            t,
        )
    });
    Course { path, end }
}

/// 纸飞机的姿态：`morph` 0 = 菱形纸片，1 = 飞机；`roll` ∈ [-1, 1] 是侧倾（压扁并换明暗面）。
struct Pose {
    at: V,
    angle: f32,
    size: f32,
    morph: f32,
    roll: f32,
    alpha: f32,
}

fn draw_plane(p: &mut Painter, f: &StyleFrame, pose: Pose) {
    let Pose {
        at,
        angle,
        size,
        morph,
        roll,
        alpha,
    } = pose;
    let (sin, cos) = angle.sin_cos();
    let squash = 1.0 - 0.5 * roll.abs();
    let world: Vec<V> = DIAMOND
        .iter()
        .zip(PLANE)
        .map(|(a, b)| {
            let (lx, ly) = (a.0 + (b.0 - a.0) * morph, a.1 + (b.1 - a.1) * morph);
            let (sx, sy) = (lx * size, ly * size * squash);
            v(at.x + sx * cos - sy * sin, at.y + sx * sin + sy * cos)
        })
        .collect();
    let (upper, lower) = if roll > 0.0 {
        (f.pal.shade, f.pal.paper)
    } else {
        (f.pal.paper, f.pal.shade)
    };
    p.fill_poly(&[world[0], world[1], world[2]], upper, alpha);
    p.fill_poly(&[world[0], world[2], world[3]], lower, alpha);
    let mut outline = world.clone();
    outline.push(world[0]);
    p.poly(&outline, 1.2, f.pal.accent, 0.9 * alpha);
    p.line(world[0], world[2], 1.0, f.pal.accent, 0.45 * alpha);
}

pub(super) fn paint(f: &StyleFrame, p: &mut Painter) {
    let course = course(f);
    let first = course.path.at(0.002).angle;
    if f.t < FOLD {
        let k = f.t / FOLD;
        let morph = smooth(seg(f.t, 60.0, FOLD));
        let at = v(f.origin.x, f.origin.y + (1.0 - out_cubic(k)) * 24.0);
        let angle = -std::f32::consts::FRAC_PI_2 + (first + std::f32::consts::FRAC_PI_2) * morph;
        let pose = Pose {
            at,
            angle,
            size: out_back(clamp01(k * 1.3)) * 1.5,
            morph,
            roll: 0.0,
            alpha: clamp01(k * 3.0),
        };
        draw_plane(p, f, pose);
    } else if f.t < LAND_AT {
        let s = in_out_sine((f.t - FOLD) / FLY);
        let here = course.path.at(s);
        let ahead = course.path.at((s + 0.03).min(1.0));
        let mut turn = ahead.angle - here.angle;
        if turn > std::f32::consts::PI {
            turn -= std::f32::consts::TAU;
        } else if turn < -std::f32::consts::PI {
            turn += std::f32::consts::TAU;
        }
        paint_dashes(f, p, &course, s);
        let pose = Pose {
            at: here.p,
            angle: here.angle,
            size: 1.5 - 0.5 * s,
            morph: 1.0,
            roll: (turn * 6.0).clamp(-1.0, 1.0),
            alpha: 1.0,
        };
        draw_plane(p, f, pose);
    } else {
        paint_landing(f, p, &course, f.t - LAND_AT);
    }
}

/// 虚线按弧长锚定，不随飞机滑动。
fn paint_dashes(f: &StyleFrame, p: &mut Painter, course: &Course, s: f32) {
    let head = s * course.path.total;
    let span = TRAIL_SPAN * course.path.total;
    let first = ((head - span).max(0.0) / DASH_PERIOD).ceil() as i32;
    let last = (head / DASH_PERIOD).floor() as i32;
    for n in first..=last {
        let start = n as f32 * DASH_PERIOD;
        let end = (start + DASH).min(head);
        if end <= start {
            continue;
        }
        let alpha = 0.55 * (1.0 - (head - start) / span);
        let points = course
            .path
            .range(start / course.path.total, end / course.path.total);
        p.poly(&points, 1.6, f.pal.accent, alpha);
    }
}

fn paint_landing(f: &StyleFrame, p: &mut Painter, course: &Course, q: f32) {
    let end_angle = course.path.at(1.0).angle;
    let shrink = seg(q, 0.0, 260.0);
    if shrink < 1.0 {
        let at = v(
            course.end.x + end_angle.cos() * 8.0 * shrink,
            course.end.y + end_angle.sin() * 8.0 * shrink,
        );
        let pose = Pose {
            at,
            angle: end_angle,
            size: 1.0 - in_cubic(shrink),
            morph: 1.0,
            roll: 0.0,
            alpha: 1.0 - shrink,
        };
        draw_plane(p, f, pose);
    }
    let k = q / END;
    let fade = 1.0 - smooth(seg(k, 0.7, 1.0));
    let pop = clamp01(k * 2.0);
    p.ring(
        course.end,
        9.0 + 14.0 * out_cubic(pop),
        1.4,
        f.pal.ok,
        0.45 * (1.0 - pop),
    );
    p.dot(course.end, 9.0 * out_back(clamp01(k * 3.0)), f.pal.ok, fade);
    let draw = clamp01(k * 3.0 - 0.4);
    if draw > 0.0 {
        let (a, b, c) = (
            v(course.end.x - 3.6, course.end.y + 0.2),
            v(course.end.x - 1.0, course.end.y + 2.8),
            v(course.end.x + 4.0, course.end.y - 2.6),
        );
        let tick = if draw < 0.4 {
            vec![a, a.lerp(b, draw / 0.4)]
        } else {
            vec![a, b, b.lerp(c, (draw - 0.4) / 0.6)]
        };
        p.poly(&tick, 1.8, 0xffffffff, fade);
    }
}
