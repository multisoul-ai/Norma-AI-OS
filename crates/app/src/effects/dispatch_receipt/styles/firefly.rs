//! 萤火：光点从指令栏升起聚拢，再各自错峰飘进卡片，落点逐个亮起。
//! 时间轴（ms）：聚拢 0–300，飘散 280–1260（每粒错开 26ms），落点绽放至 1580。

use super::painter::*;
use super::StyleFrame;

const COUNT: u32 = 14;
const START: f32 = 280.0;
const FLIGHT: f32 = 640.0;
const BLOOM: f32 = 320.0;
const STAGGER: f32 = 26.0;
const TRAIL: u32 = 5;

pub(super) fn paint(f: &StyleFrame, p: &mut Painter) {
    let k = p.scale;
    let origin = v(f.origin.x, f.origin.y - 6.0 * k);
    for i in 0..COUNT {
        let rnd = |slot: u32| unit(f.seed, i * 8 + slot);
        let angle = -std::f32::consts::FRAC_PI_2 + (rnd(0) - 0.5) * std::f32::consts::PI * 1.3;
        let radius = (12.0 + rnd(1) * 30.0) * k;
        let cloud = v(
            origin.x + angle.cos() * radius * 1.4,
            origin.y - 26.0 * k + angle.sin() * radius * 0.8,
        );
        let land = v(
            f.card.x + 22.0 * k + rnd(2) * (f.card.w - 44.0 * k).max(1.0),
            f.card.y + 34.0 * k + rnd(3) * (f.card.h - 54.0 * k).max(10.0 * k),
        );
        let ctrl = bow_control(cloud, land, (rnd(4) - 0.5) * 0.7, 220.0 * k);
        let size = 1.5 + rnd(5) * 1.1;
        let phase = rnd(6) * std::f32::consts::TAU;
        let gather_delay = rnd(7) * 120.0;
        let rank = (i * 5 + (f.seed % u64::from(COUNT)) as u32) % COUNT;

        let gather = out_cubic(seg(f.t, gather_delay * 0.5, 300.0));
        if gather <= 0.0 {
            continue;
        }
        let start = START + rank as f32 * STAGGER;
        let progress = clamp01((f.t - start) / FLIGHT);
        let flicker = 0.72 + 0.28 * (f.t * 0.022 + phase).sin();
        if progress < 1.0 {
            let eased = in_out_cubic(progress);
            let wobble = v(
                (f.t * 0.006 + phase).sin() * 2.5 * k,
                (f.t * 0.005 + phase).cos() * 2.0 * k,
            );
            let base = if progress > 0.0 {
                quad_bezier(cloud, ctrl, land, eased)
            } else {
                origin.lerp(cloud, gather)
            };
            let at = v(
                base.x + wobble.x * (1.0 - eased),
                base.y + wobble.y * (1.0 - eased),
            );
            if progress > 0.0 {
                for step in 1..=TRAIL {
                    let behind = quad_bezier(
                        cloud,
                        ctrl,
                        land,
                        in_out_cubic((progress - 0.028 * step as f32).max(0.0)),
                    );
                    let fade = 1.0 - step as f32 / (TRAIL + 1) as f32;
                    p.dot(behind, size * fade, f.pal.gold, 0.3 * fade * flicker);
                }
            }
            p.glow(at, size * 4.5, f.pal.gold, 0.5 * flicker * gather);
            p.dot(at, size, f.pal.gold, 0.95 * flicker * gather);
        } else {
            let bloom = seg(f.t, start + FLIGHT, start + FLIGHT + BLOOM);
            if bloom < 1.0 {
                p.ring(
                    land,
                    2.0 + 11.0 * out_cubic(bloom),
                    1.2,
                    f.pal.gold,
                    0.7 * (1.0 - bloom),
                );
                p.dot(land, size * (1.0 - bloom), f.pal.gold, 1.0 - bloom);
            }
        }
    }
}
