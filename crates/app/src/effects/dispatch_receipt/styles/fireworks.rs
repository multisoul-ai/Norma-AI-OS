//! 烟花：火箭拖着金屑升空，在卡片上方炸成双色菊花，余烬噼啪闪烁着落下。
//! 粒子位置是时间的解析函数（阻力 + 重力），不保存状态。
//! 时间轴（ms）：升空 0–520，炸开 520–，余烬到 1900。

use super::painter::*;
use super::StyleFrame;

const LAUNCH: f32 = 520.0;
const OUTER: u32 = 56;
const INNER: u32 = 20;
const SPARKS: u32 = 18;
const CRACKLE_AT: f32 = 0.65;
const CRACKLE_PARENTS: f32 = 12.0;

struct Star {
    velocity: V,
    life: f32,
    drag: f32,
    gravity: f32,
    size: f32,
    twinkle: bool,
    outer: bool,
}

fn star(seed: u64, i: u32, k: f32) -> Star {
    let u = |slot: u32| unit(seed, 1000 + i * 4 + slot);
    if i < OUTER {
        let angle = i as f32 / OUTER as f32 * std::f32::consts::TAU + (u(0) - 0.5) * 0.06;
        let speed = 230.0 * (0.85 + 0.15 * u(1)) * k;
        Star {
            velocity: v(angle.cos() * speed, angle.sin() * speed),
            life: 1.0 + u(2) * 0.3,
            drag: 2.4,
            gravity: 110.0 * k,
            size: 1.7,
            twinkle: true,
            outer: true,
        }
    } else {
        let angle = u(0) * std::f32::consts::TAU;
        let speed = (60.0 + u(1) * 70.0) * k;
        Star {
            velocity: v(angle.cos() * speed, angle.sin() * speed),
            life: 0.7 + u(2) * 0.3,
            drag: 2.8,
            gravity: 90.0 * k,
            size: 1.4,
            twinkle: false,
            outer: false,
        }
    }
}

/// 阻力 `drag` 下的速度衰减积分，加上重力项；`age` 单位秒。
fn position(burst: V, s: &Star, age: f32) -> V {
    let eased = 1.0 - (-s.drag * age).exp();
    v(
        burst.x + s.velocity.x / s.drag * eased,
        burst.y + s.velocity.y / s.drag * eased + s.gravity / s.drag * (age - eased / s.drag),
    )
}

pub(super) fn paint(f: &StyleFrame, p: &mut Painter) {
    let k = p.scale;
    let from = f.origin;
    // 爆点放在卡片内部偏上：画在卡片上方，且不会因为卡片贴近窗口顶部而被截掉。
    let burst = v(
        f.card.center().x,
        f.card.y + (f.card.h * 0.3).min(220.0 * k.max(0.5)),
    );
    let ctrl = v(
        from.x + (burst.x - from.x) * 0.15,
        burst.y + (from.y - burst.y) * 0.25,
    );
    let rocket = |t: f32| quad_bezier(from, ctrl, burst, 1.0 - (1.0 - clamp01(t / LAUNCH)).powi(2));
    paint_rocket(f, p, &rocket);
    if f.t >= LAUNCH {
        paint_burst(f, p, burst);
    }
}

fn paint_rocket(f: &StyleFrame, p: &mut Painter, rocket: &dyn Fn(f32) -> V) {
    let k = p.scale;
    for i in 0..SPARKS {
        let emitted = LAUNCH * (i as f32 + 0.5) / SPARKS as f32;
        let age = (f.t - emitted) / 1000.0;
        let life = 0.35 + unit(f.seed, 300 + i) * 0.25;
        if age < 0.0 || age >= life {
            continue;
        }
        let at = rocket(emitted);
        let drift = v(
            (unit(f.seed, 340 + i) - 0.5) * 40.0 * k * age,
            (20.0 + unit(f.seed, 380 + i) * 40.0) * k * age + 60.0 * k * age * age,
        );
        p.dot(
            v(at.x + drift.x, at.y + drift.y),
            1.1,
            f.pal.gold,
            0.9 * (1.0 - age / life),
        );
    }
    if f.t < LAUNCH {
        let head = rocket(f.t);
        p.line(rocket((f.t - 26.0).max(0.0)), head, 2.2, f.pal.gold, 0.8);
        p.glow(head, 12.0, f.pal.gold, 0.7);
        p.dot(head, 1.9, f.pal.core, 1.0);
    }
}

fn paint_burst(f: &StyleFrame, p: &mut Painter, burst: V) {
    let k = p.scale;
    let tone = [f.pal.gold, f.pal.rose, f.pal.cyan];
    let (outer_color, inner_color) = (
        tone[(f.seed % 3) as usize],
        tone[((f.seed / 3) % 2 + 1 + f.seed % 3) as usize % 3],
    );
    let age = (f.t - LAUNCH) / 1000.0;
    let flash = 1.0 - seg(f.t, LAUNCH, LAUNCH + 260.0);
    let expand = out_cubic(seg(f.t, LAUNCH, LAUNCH + 500.0));
    p.glow(
        burst,
        70.0 * k * (0.6 + 0.4 * (1.0 - flash)),
        f.pal.core,
        0.55 * flash * flash,
    );
    p.ring(
        burst,
        (10.0 + 90.0 * expand) * k,
        1.5,
        outer_color,
        0.4 * (1.0 - expand),
    );
    let settle = seg(f.t, 600.0, 1500.0);
    p.card_outline(
        f.card,
        2.0 + 8.0 * out_cubic(seg(f.t, 600.0, 900.0)),
        1.6,
        f.pal.gold,
        0.55 * (1.0 - settle),
        0.0,
    );

    for i in 0..OUTER + INNER {
        let s = star(f.seed, i, k);
        let life_k = age / s.life;
        if life_k >= 1.0 {
            continue;
        }
        let mut alpha = if life_k < 0.7 {
            1.0
        } else {
            1.0 - (life_k - 0.7) / 0.3
        };
        if s.twinkle && life_k > 0.55 && unit(f.seed, 5000 + i * 131 + (f.t / 45.0) as u32) < 0.35 {
            alpha *= 0.15;
        }
        let color = if s.outer { outer_color } else { inner_color };
        let head = position(burst, &s, age);
        let trail = [
            position(burst, &s, (age - 0.03).max(0.0)),
            position(burst, &s, (age - 0.07).max(0.0)),
        ];
        p.poly(
            &[head, trail[0], trail[1]],
            s.size * 1.5,
            color,
            0.7 * alpha,
        );
        if s.outer && i % 4 == 0 {
            p.glow(head, s.size * 5.0, color, 0.3 * alpha);
        }
        let hot = age < 0.15;
        p.dot(head, s.size, if hot { f.pal.core } else { color }, alpha);
        if s.outer && age > CRACKLE_AT && unit(f.seed, 7000 + i) < CRACKLE_PARENTS / OUTER as f32 {
            paint_crackle(f, p, burst, &s, i, age);
        }
    }
}

/// 外圈几颗在 `CRACKLE_AT` 秒时再碎成小白点。
fn paint_crackle(f: &StyleFrame, p: &mut Painter, burst: V, parent: &Star, i: u32, age: f32) {
    let k = p.scale;
    let born = position(burst, parent, CRACKLE_AT);
    let since = age - CRACKLE_AT;
    if since >= 0.22 {
        return;
    }
    for j in 0..6 {
        let angle = unit(f.seed, 9000 + i * 6 + j) * std::f32::consts::TAU;
        let speed = (40.0 + unit(f.seed, 9500 + i * 6 + j) * 50.0) * k;
        let at = v(
            born.x + angle.cos() * speed * since,
            born.y + angle.sin() * speed * since + 20.0 * k * since * since,
        );
        p.dot(at, 0.9, f.pal.core, 1.0 - since / 0.22);
    }
}
