//! 星漩：星尘在指令栏旋成小星系，拖着螺旋尾巴飞向卡片，绕卡片转一圈后坍缩成闪光。
//! 每颗星尘的位置是时间的解析函数，滞后量不同拉出螺旋尾巴。
//! 时间轴（ms）：聚成星盘 0–360，飞行 340–980，撑开 980–1180，坍缩 1180–1420，闪光至 1760。

use super::painter::*;
use super::StyleFrame;

const COUNT: u32 = 64;
const GATHER: f32 = 360.0;
const TRAVEL: (f32, f32) = (340.0, 980.0);
const OPEN: f32 = 200.0;
const COLLAPSE: (f32, f32) = (1180.0, 1420.0);
const END: f32 = 1760.0;

struct Dust {
    radius: f32,
    angle: f32,
    lag: f32,
    size: f32,
    warm: bool,
    reach: f32,
    spin: f32,
    big: bool,
}

fn dust(seed: u64, i: u32) -> Dust {
    let u = |slot: u32| unit(seed, i * 8 + slot);
    Dust {
        radius: 46.0 + u(0) * 74.0,
        angle: u(1) * std::f32::consts::TAU,
        lag: u(2).powf(1.5) * 0.32,
        size: 0.8 + u(3) * 1.4,
        warm: u(4) < 0.5,
        reach: 0.6 + u(5) * 0.5,
        spin: 0.8 + u(6) * 0.5,
        big: i.is_multiple_of(6),
    }
}

fn position(d: &Dust, t: f32, path: &Poly, card: Rect, k: f32) -> V {
    let local = t - d.lag * 620.0;
    let center = path.at(in_out_cubic(seg(local, TRAVEL.0, TRAVEL.1))).p;
    let radius = d.radius
        * k
        * (1.0 - 0.72 * out_cubic(t / GATHER))
        * (1.0 + 0.18 * (t * 0.004 + d.angle).sin());
    let (mut rx, mut ry) = (radius, radius * 0.42);
    let open = out_cubic(seg(local, TRAVEL.1, TRAVEL.1 + OPEN));
    rx += (card.w * 0.5 * d.reach - rx) * open;
    ry += (card.h * 0.5 * d.reach - ry) * open;
    let fall = in_cubic(seg(t, COLLAPSE.0, COLLAPSE.1));
    let theta = d.angle + t * 0.005 * d.spin + (t / 1000.0).powi(2) * 5.0 * d.spin + fall * 6.0;
    v(
        center.x + theta.cos() * rx * (1.0 - fall),
        center.y + theta.sin() * ry * (1.0 - fall),
    )
}

pub(super) fn paint(f: &StyleFrame, p: &mut Painter) {
    let k = p.scale;
    let from = v(f.origin.x, f.origin.y - 30.0 * k);
    let target = f.card.center();
    let path = Poly::sampled(90, |t| {
        quad_bezier(from, bow_control(from, target, 0.22, 170.0), target, t)
    });
    let fade_in = out_cubic(seg(f.t, 0.0, 220.0));
    let alive = 1.0 - seg(f.t, COLLAPSE.1, COLLAPSE.1 + 60.0);
    if alive > 0.0 {
        let core = path.at(in_out_cubic(seg(f.t, TRAVEL.0, TRAVEL.1))).p;
        let hold = fade_in * (1.0 - seg(f.t, TRAVEL.1, TRAVEL.1 + 200.0));
        p.glow(core, 28.0 * k, f.pal.violet, 0.5 * hold);
        p.glow(core, 12.0 * k, f.pal.core, 0.6 * hold);
        for i in 0..COUNT {
            let d = dust(f.seed, i);
            let (now, before) = (
                position(&d, f.t, &path, f.card, k),
                position(&d, f.t - 18.0, &path, f.card, k),
            );
            let color = if d.warm { f.pal.cyan } else { f.pal.violet };
            p.line(before, now, d.size * 1.3, color, 0.85 * fade_in * alive);
            p.dot(now, d.size * 0.7, f.pal.core, 0.9 * fade_in * alive);
            if d.big {
                p.glow(now, 7.0 * k, color, 0.3 * fade_in * alive);
            }
        }
    }
    let q = seg(f.t, COLLAPSE.1, END);
    if q > 0.0 && q < 1.0 {
        let e = out_cubic(q);
        p.glow(
            target,
            (30.0 + 70.0 * e) * k,
            f.pal.core,
            0.7 * (1.0 - q) * (1.0 - q),
        );
        p.ring(
            target,
            (8.0 * k + f.card.w * 0.6 * e) / k.max(0.01),
            2.0 * (1.0 - q),
            f.pal.glow,
            0.6 * (1.0 - q),
        );
        p.card_outline(
            f.card,
            2.0 + 10.0 * e,
            1.4,
            f.pal.cyan,
            0.7 * (1.0 - q),
            0.0,
        );
        for n in 0..8 {
            let a = n as f32 * std::f32::consts::FRAC_PI_4 + std::f32::consts::FRAC_PI_8;
            let (r, l) = ((14.0 + 60.0 * e) * k, (1.0 - q) * 34.0 * k);
            p.line(
                v(target.x + a.cos() * r, target.y + a.sin() * r),
                v(target.x + a.cos() * (r + l), target.y + a.sin() * (r + l)),
                1.4,
                f.pal.core,
                0.7 * (1.0 - q),
            );
        }
    }
}
