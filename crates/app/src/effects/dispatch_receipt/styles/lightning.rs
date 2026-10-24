//! 闪电：指令栏蓄电，一道带分叉的闪电劈向卡片，连闪两下，卡片边缘爬满电弧。
//! 折线用中点位移法生成，随机数全部来自 (种子, 序号) 哈希，同一次派发形状固定。
//! 时间轴（ms）：蓄电 0–180，劈下 160–330，余辉与电弧至 760。

use super::painter::*;
use super::StyleFrame;

const STRIKES: [f32; 2] = [160.0, 270.0];
const ARC_STEP: f32 = 45.0;

/// 中点位移：每层把每段中点沿法线推开 `disp`，幅度逐层减小。
fn bolt(seed: u64, salt: u32, a: V, b: V, mut disp: f32, depth: u32) -> Vec<V> {
    let mut points = vec![a, b];
    let mut counter = 0;
    for _ in 0..depth {
        let mut next = vec![points[0]];
        for pair in points.windows(2) {
            let (p, q) = (pair[0], pair[1]);
            let (dx, dy) = (q.x - p.x, q.y - p.y);
            let len = dx.hypot(dy).max(1e-3);
            let offset = (unit(seed, salt.wrapping_mul(4096).wrapping_add(counter)) - 0.5) * disp;
            counter += 1;
            next.push(v(
                (p.x + q.x) / 2.0 - dy / len * offset,
                (p.y + q.y) / 2.0 + dx / len * offset,
            ));
            next.push(q);
        }
        points = next;
        disp *= 0.55;
    }
    points
}

fn envelope(t: f32) -> f32 {
    match t {
        t if t < 160.0 => 0.0,
        t if t < 230.0 => 1.0,
        t if t < 270.0 => 0.25,
        t if t < 330.0 => 1.0,
        t => 1.0 - out_cubic(seg(t, 330.0, 620.0)),
    }
}

pub(super) fn paint(f: &StyleFrame, p: &mut Painter) {
    let k = p.scale;
    let from = v(f.origin.x, f.origin.y - 2.0);
    // 劈在卡片中心、画在卡片上方（落点在卡片下边缘会被指令栏或卡片本身遮住）。
    let end = f.card.center();
    if f.t < 180.0 {
        charge(f, p, from);
    }
    let level = envelope(f.t);
    if level > 0.0 {
        let strike = u32::from(f.t >= STRIKES[1]);
        paint_bolt(f, p, from, end, strike, level);
        p.glow(end, 42.0 * k, f.pal.glow, 0.6 * level);
    }
    for (n, &at) in STRIKES.iter().enumerate() {
        paint_sparks(f, p, end, at, n as u32);
    }
    if (200.0..720.0).contains(&f.t) {
        paint_arcs(f, p);
    }
}

fn charge(f: &StyleFrame, p: &mut Painter, from: V) {
    let k = p.scale;
    let g = f.t / 180.0;
    p.glow(from, (10.0 + 16.0 * g) * k, f.pal.violet, 0.55 * g);
    let bucket = (f.t / ARC_STEP) as u32;
    for n in 0..2 {
        let angle = unit(f.seed, 60 + bucket * 2 + n) * std::f32::consts::TAU;
        let r = (8.0 + 14.0 * g) * k;
        let tip = v(from.x + angle.cos() * r, from.y + angle.sin() * r);
        p.poly(
            &bolt(f.seed, 900 + bucket * 2 + n, from, tip, 6.0 * k, 3),
            1.0,
            f.pal.core,
            0.8,
        );
    }
}

fn paint_bolt(f: &StyleFrame, p: &mut Painter, from: V, end: V, strike: u32, level: f32) {
    let len = from.dist(end);
    let salt = 100 + strike * 50;
    let main = bolt(f.seed, salt, from, end, len * 0.22, 6);
    let thin = if f.t < 330.0 {
        1.0
    } else {
        1.0 - seg(f.t, 330.0, 620.0) * 0.6
    };
    p.poly(&main, 12.0, f.pal.violet, 0.13 * level);
    p.poly(&main, 4.5, f.pal.glow, 0.38 * level);
    p.poly(&main, 1.8 * thin, f.pal.core, level);
    for b in 0..3 {
        let u = |slot: u32| unit(f.seed, 2000 + strike * 100 + b * 8 + slot);
        let start =
            main[10 + ((main.len() as f32 - 10.0) * 0.55 * u(0)) as usize % (main.len() - 10)];
        let angle = (end.y - from.y).atan2(end.x - from.x)
            + if u(1) < 0.5 { -1.0 } else { 1.0 } * (0.4 + u(2) * 0.5);
        let reach = len * (0.12 + u(3) * 0.13);
        let tip = v(start.x + angle.cos() * reach, start.y + angle.sin() * reach);
        let branch = bolt(f.seed, salt + 10 + b, start, tip, reach * 0.3, 4);
        p.poly(&branch, 6.0, f.pal.violet, 0.1 * level);
        p.poly(&branch, 2.4, f.pal.glow, 0.3 * level);
        p.poly(&branch, 1.0 * thin, f.pal.core, 0.8 * level);
    }
}

fn paint_sparks(f: &StyleFrame, p: &mut Painter, end: V, at: f32, strike: u32) {
    let k = p.scale;
    let age = (f.t - at) / 1000.0;
    if age < 0.0 {
        return;
    }
    for i in 0..16 {
        let u = |slot: u32| unit(f.seed, 3000 + strike * 100 + i * 4 + slot);
        let life = 0.3 + u(0) * 0.25;
        if age >= life {
            continue;
        }
        let angle = -std::f32::consts::PI * (0.1 + u(1) * 0.8);
        let speed = (120.0 + u(2) * 180.0) * k;
        let pos = |a: f32| {
            v(
                end.x + angle.cos() * speed * a,
                end.y + angle.sin() * speed * a + 150.0 * k * a * a,
            )
        };
        p.line(
            pos((age - 0.03).max(0.0)),
            pos(age),
            1.4,
            f.pal.core,
            1.0 - age / life,
        );
    }
}

fn paint_arcs(f: &StyleFrame, p: &mut Painter) {
    let k = p.scale;
    let fade = 1.0 - seg(f.t, 200.0, 720.0);
    let bucket = ((f.t - 200.0) / ARC_STEP) as u32;
    p.card_outline(f.card, 0.0, 1.5, f.pal.glow, 0.8 * fade, 0.0);
    let (x, y, w, h) = (f.card.x, f.card.y, f.card.w, f.card.h);
    for n in 0..4 {
        let u = |slot: u32| unit(f.seed, 4000 + bucket * 16 + n * 4 + slot);
        let along = u(0);
        let (start, out) = match (along * 4.0) as u32 {
            0 => (v(x + w * (along * 4.0).fract(), y), v(0.0, -1.0)),
            1 => (v(x + w, y + h * (along * 4.0).fract()), v(1.0, 0.0)),
            2 => (v(x + w * (1.0 - (along * 4.0).fract()), y + h), v(0.0, 1.0)),
            _ => (v(x, y + h * (1.0 - (along * 4.0).fract())), v(-1.0, 0.0)),
        };
        let reach = (18.0 + u(1) * 22.0) * k;
        let tip = v(start.x + out.x * reach, start.y + out.y * reach);
        let arc = bolt(f.seed, 800 + bucket * 4 + n, start, tip, 10.0 * k, 3);
        p.poly(&arc, 4.0, f.pal.glow, 0.25 * fade);
        p.poly(&arc, 1.2, f.pal.core, fade);
    }
}
