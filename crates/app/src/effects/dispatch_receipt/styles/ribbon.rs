//! 声纹：说话的波形从指令栏展开成丝带，送到卡片底边，化成一排跳动的音柱。
//! 时间轴（ms）：丝带 0–720（头部先到、尾部收拢），音柱 660–1360。

use super::painter::*;
use super::StyleFrame;

const RUN: f32 = 720.0;
const BARS_AT: f32 = 660.0;
const BARS: u32 = 20;
const POINTS: u32 = 48;
const CHUNKS: u32 = 3;
/// (线宽, 透明度, 振幅系数)：主线最亮，另两条错相做出丝带的厚度。
const WAVES: [(f32, f32, f32); 3] = [(2.4, 0.95, 1.0), (1.6, 0.6, 0.62), (1.1, 0.45, 0.38)];

fn mix(a: u32, b: u32, t: f32) -> u32 {
    let channel = |shift: u32| {
        let (x, y) = (((a >> shift) & 0xff) as f32, ((b >> shift) & 0xff) as f32);
        ((x + (y - x) * t).round() as u32) << shift
    };
    channel(24) | channel(16) | channel(8) | 0xff
}

pub(super) fn paint(f: &StyleFrame, p: &mut Painter) {
    let from = v(f.origin.x, f.origin.y);
    // 落在卡片中心、画在卡片上方：卡片很高时下边缘贴着指令栏，会被遮住。
    let end = f.card.center();
    let path = Poly::sampled(100, |t| {
        quad_bezier(from, bow_control(from, end, 0.16, 170.0), end, t)
    });
    let run = clamp01(f.t / RUN);
    let head = out_cubic(clamp01(run / 0.72));
    let tail = in_cubic(clamp01((run - 0.28) / 0.72));
    if head - tail > 0.003 {
        paint_ribbon(f, p, &path, tail, head);
    }
    if f.t >= BARS_AT {
        paint_bars(f, p, end);
    }
}

fn paint_ribbon(f: &StyleFrame, p: &mut Painter, path: &Poly, tail: f32, head: f32) {
    for (layer, &(width, alpha, amp)) in WAVES.iter().enumerate() {
        let wave: Vec<(V, f32)> = (0..=POINTS)
            .map(|i| {
                let loc = i as f32 / POINTS as f32;
                let u = tail + (head - tail) * loc;
                let at = path.at(u);
                let envelope = (std::f32::consts::PI * loc).sin().max(0.0).powf(0.8)
                    * (1.0 - u).powf(0.5)
                    * (0.45 + 0.55 * ((u * 19.0 + 1.3).sin() * (u * 6.2).cos()).abs());
                let phase =
                    u * path.total / 28.0 * std::f32::consts::TAU - f.t * 0.02 + layer as f32 * 2.1;
                let offset = 13.0 * envelope * amp * phase.sin();
                (
                    v(
                        at.p.x - at.angle.sin() * offset,
                        at.p.y + at.angle.cos() * offset,
                    ),
                    u,
                )
            })
            .collect();
        for chunk in 0..CHUNKS {
            let (from, to) = (
                (POINTS * chunk / CHUNKS) as usize,
                (POINTS * (chunk + 1) / CHUNKS) as usize,
            );
            let points: Vec<V> = wave[from..=to].iter().map(|(pt, _)| *pt).collect();
            let color = mix(f.pal.cyan, f.pal.violet, wave[(from + to) / 2].1);
            p.poly(&points, width, color, alpha);
            if layer == 0 {
                p.poly(&points, 8.0, color, 0.12);
            }
        }
    }
}

fn paint_bars(f: &StyleFrame, p: &mut Painter, end: V) {
    let q = seg(f.t, BARS_AT, BARS_AT + 700.0);
    let envelope = if q < 0.18 {
        out_cubic(q / 0.18)
    } else {
        1.0 - in_cubic((q - 0.18) / 0.82)
    };
    for i in 0..BARS {
        let x = f.card.x + 20.0 + (f.card.w - 40.0) * i as f32 / (BARS - 1) as f32;
        let nearness = 1.0 - ((x - end.x).abs() / (f.card.w * 0.6)).min(1.0);
        let bounce = 0.4 + 0.6 * (f.t * 0.03 + unit(f.seed, i) * 6.0).sin().abs();
        // 音柱高度随卡片大小放大，整张卡片上也看得清。
        let peak = (f.card.h * 0.1).clamp(22.0 * p.scale, 72.0);
        let height = (4.0 + peak * nearness * bounce) * envelope;
        let color = mix(f.pal.cyan, f.pal.violet, i as f32 / (BARS - 1) as f32);
        p.line(
            v(x, end.y - height / 2.0),
            v(x, end.y + height / 2.0),
            2.4,
            color,
            0.95,
        );
    }
    p.card_outline(f.card, 0.0, 6.0, f.pal.cyan, 0.1 * envelope, 0.0);
    p.card_outline(f.card, 0.0, 1.5, f.pal.cyan, 0.7 * envelope, 0.0);
}
