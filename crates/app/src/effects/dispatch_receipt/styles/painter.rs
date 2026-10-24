//! 新款式共用的画笔与数学工具。
//!
//! 所有款式都是 (几何, 基准时间 ms, 种子) 的纯函数：不保存粒子状态，重放、定格预览和测试结果一致。
//! GPUI 0.2.2 没有加法混合与模糊，辉光用同心半透明圆叠出来。

use super::super::primitives::{outline_quad, round_quad, with_alpha};
use super::super::DispatchPaintPlan;
use gpui::{point, px, rgba, Bounds, PathBuilder, Pixels, Point};
use ui::Theme;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct V {
    pub x: f32,
    pub y: f32,
}

pub(super) fn v(x: f32, y: f32) -> V {
    V { x, y }
}

impl V {
    pub(super) fn lerp(self, other: V, t: f32) -> V {
        v(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        )
    }

    pub(super) fn dist(self, other: V) -> f32 {
        (other.x - self.x).hypot(other.y - self.y)
    }

    fn to_point(self) -> Point<Pixels> {
        point(px(self.x), px(self.y))
    }
}

impl From<Point<Pixels>> for V {
    fn from(p: Point<Pixels>) -> Self {
        v(f32::from(p.x), f32::from(p.y))
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl From<Bounds<Pixels>> for Rect {
    fn from(b: Bounds<Pixels>) -> Self {
        Rect {
            x: f32::from(b.origin.x),
            y: f32::from(b.origin.y),
            w: f32::from(b.size.width),
            h: f32::from(b.size.height),
        }
    }
}

impl Rect {
    pub(super) fn center(self) -> V {
        v(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

// ---------- 缓动与插值 ----------

pub(super) fn clamp01(t: f32) -> f32 {
    t.clamp(0.0, 1.0)
}

/// `t` 在 [a, b] 内的归一化进度。
pub(super) fn seg(t: f32, a: f32, b: f32) -> f32 {
    clamp01((t - a) / (b - a))
}

pub(super) fn out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - clamp01(t)).powi(3)
}

pub(super) fn in_cubic(t: f32) -> f32 {
    clamp01(t).powi(3)
}

pub(super) fn in_out_cubic(t: f32) -> f32 {
    let t = clamp01(t);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

pub(super) fn in_out_sine(t: f32) -> f32 {
    -((std::f32::consts::PI * clamp01(t)).cos() - 1.0) / 2.0
}

pub(super) fn out_back(t: f32) -> f32 {
    let k = 1.70158;
    let t = clamp01(t) - 1.0;
    1.0 + (k + 1.0) * t.powi(3) + k * t * t
}

pub(super) fn smooth(t: f32) -> f32 {
    let t = clamp01(t);
    t * t * (3.0 - 2.0 * t)
}

// ---------- 确定性随机 ----------

/// [0, 1) 的哈希，同一 (seed, index) 永远得到同一个值。
pub(super) fn unit(seed: u64, index: u32) -> f32 {
    let mut x = seed ^ (u64::from(index).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    ((x >> 40) as f32) / (1u64 << 24) as f32
}

// ---------- 曲线 ----------

pub(super) fn quad_bezier(a: V, k: V, b: V, t: f32) -> V {
    let u = 1.0 - t;
    v(
        u * u * a.x + 2.0 * u * t * k.x + t * t * b.x,
        u * u * a.y + 2.0 * u * t * k.y + t * t * b.y,
    )
}

pub(super) fn cubic_bezier(a: V, k1: V, k2: V, b: V, t: f32) -> V {
    let u = 1.0 - t;
    v(
        u * u * u * a.x + 3.0 * u * u * t * k1.x + 3.0 * u * t * t * k2.x + t * t * t * b.x,
        u * u * u * a.y + 3.0 * u * u * t * k1.y + 3.0 * u * t * t * k2.y + t * t * t * b.y,
    )
}

/// 弦的垂线方向弯一下的控制点；`k` 的符号决定朝哪边弯，最大弯曲量 `max`。
pub(super) fn bow_control(a: V, b: V, k: f32, max: f32) -> V {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let d = dx.hypot(dy).max(1.0);
    let bow = (d * k.abs()).min(max) * k.signum() * if dx >= 0.0 { 1.0 } else { -1.0 };
    v(
        (a.x + b.x) / 2.0 - dy / d * bow,
        (a.y + b.y) / 2.0 + dx / d * bow,
    )
}

/// 按弧长取点的折线：`at(u)` 取点与切线角，`range(u0, u1)` 取一段。
pub(super) struct Poly {
    points: Vec<V>,
    lengths: Vec<f32>,
    pub total: f32,
}

pub(super) struct PolyPoint {
    pub p: V,
    pub angle: f32,
}

impl Poly {
    pub(super) fn new(points: Vec<V>) -> Self {
        let mut lengths = vec![0.0];
        for pair in points.windows(2) {
            let last = *lengths.last().unwrap_or(&0.0);
            lengths.push(last + pair[0].dist(pair[1]));
        }
        let total = lengths.last().copied().unwrap_or(0.0).max(f32::EPSILON);
        Self {
            points,
            lengths,
            total,
        }
    }

    pub(super) fn sampled(count: usize, f: impl Fn(f32) -> V) -> Self {
        Self::new((0..=count).map(|i| f(i as f32 / count as f32)).collect())
    }

    pub(super) fn at(&self, u: f32) -> PolyPoint {
        let d = clamp01(u) * self.total;
        let i = self
            .lengths
            .partition_point(|&len| len < d)
            .clamp(1, self.points.len().saturating_sub(1).max(1))
            - 1;
        let a = self.points[i];
        let b = *self.points.get(i + 1).unwrap_or(&a);
        let span =
            (self.lengths[(i + 1).min(self.lengths.len() - 1)] - self.lengths[i]).max(f32::EPSILON);
        PolyPoint {
            p: a.lerp(b, (d - self.lengths[i]) / span),
            angle: (b.y - a.y).atan2(b.x - a.x),
        }
    }

    pub(super) fn range(&self, u0: f32, u1: f32) -> Vec<V> {
        let (u0, u1) = (clamp01(u0), clamp01(u1));
        if u1 <= u0 {
            return Vec::new();
        }
        let (d0, d1) = (u0 * self.total, u1 * self.total);
        let mut out = vec![self.at(u0).p];
        out.extend(
            self.points
                .iter()
                .zip(&self.lengths)
                .filter(|(_, &len)| len > d0 && len < d1)
                .map(|(p, _)| *p),
        );
        out.push(self.at(u1).p);
        out
    }
}

// ---------- 调色板 ----------

#[derive(Clone, Copy)]
pub(super) struct Palette {
    pub core: u32,
    pub glow: u32,
    pub cyan: u32,
    pub violet: u32,
    pub gold: u32,
    pub rose: u32,
    pub ok: u32,
    pub paper: u32,
    pub shade: u32,
    pub accent: u32,
}

impl Palette {
    /// 深色主题用高亮色，浅色主题用饱和色，对应原型的两套 token。
    pub(super) fn for_theme(theme: Theme) -> Self {
        let dark = luminance(theme.text_primary) > 0.5;
        if dark {
            Self {
                core: 0xf2f9ffff,
                glow: 0x96cdffff,
                cyan: 0x27d7eaff,
                violet: 0x8f86ffff,
                gold: 0xffd37aff,
                rose: 0xff7aa8ff,
                ok: 0x64e0b5ff,
                paper: 0xf3f6f9ff,
                shade: 0xaebdccff,
                accent: theme.accent_blue,
            }
        } else {
            Self {
                core: 0x1f6fe0ff,
                glow: 0x7fb2f7ff,
                cyan: 0x0fa3bdff,
                violet: 0x6a5cffff,
                gold: 0xe08a00ff,
                rose: 0xe5487dff,
                ok: 0x1f9e72ff,
                paper: 0xffffffff,
                shade: 0xdbe6f2ff,
                accent: theme.accent_blue,
            }
        }
    }
}

fn luminance(color: u32) -> f32 {
    let channel = |shift: u32| ((color >> shift) & 0xff) as f32 / 255.0;
    0.2126 * channel(24) + 0.7152 * channel(16) + 0.0722 * channel(8)
}

// ---------- 画笔 ----------

fn alpha8(alpha: f32) -> u8 {
    (clamp01(alpha) * 255.0).round() as u8
}

pub(super) struct Painter<'a> {
    pub plan: &'a mut DispatchPaintPlan,
    pub scale: f32,
}

impl Painter<'_> {
    pub(super) fn poly(&mut self, points: &[V], width: f32, color: u32, alpha: f32) {
        if points.len() < 2 || alpha <= 0.0 {
            return;
        }
        let mut builder = PathBuilder::stroke(px((width * self.scale).max(0.25)));
        builder.move_to(points[0].to_point());
        for p in &points[1..] {
            builder.line_to(p.to_point());
        }
        if let Ok(path) = builder.build() {
            self.plan
                .paths
                .push((path, rgba(with_alpha(color, alpha8(alpha))).into()));
        }
    }

    pub(super) fn line(&mut self, a: V, b: V, width: f32, color: u32, alpha: f32) {
        self.poly(&[a, b], width, color, alpha);
    }

    pub(super) fn fill_poly(&mut self, points: &[V], color: u32, alpha: f32) {
        if points.len() < 3 || alpha <= 0.0 {
            return;
        }
        let mut builder = PathBuilder::fill();
        builder.move_to(points[0].to_point());
        for p in &points[1..] {
            builder.line_to(p.to_point());
        }
        builder.close();
        if let Ok(path) = builder.build() {
            self.plan
                .paths
                .push((path, rgba(with_alpha(color, alpha8(alpha))).into()));
        }
    }

    pub(super) fn dot(&mut self, c: V, radius: f32, color: u32, alpha: f32) {
        let r = radius * self.scale;
        if r <= 0.05 || alpha <= 0.0 {
            return;
        }
        let bounds = Bounds {
            origin: point(px(c.x - r), px(c.y - r)),
            size: gpui::Size {
                width: px(r * 2.0),
                height: px(r * 2.0),
            },
        };
        self.plan
            .quads
            .push(round_quad(bounds, r, with_alpha(color, alpha8(alpha))));
    }

    pub(super) fn ring(&mut self, c: V, radius: f32, width: f32, color: u32, alpha: f32) {
        let r = radius * self.scale;
        if r <= 0.1 || width <= 0.0 || alpha <= 0.0 {
            return;
        }
        let bounds = Bounds {
            origin: point(px(c.x - r), px(c.y - r)),
            size: gpui::Size {
                width: px(r * 2.0),
                height: px(r * 2.0),
            },
        };
        self.plan.quads.push(outline_quad(
            bounds,
            r,
            with_alpha(color, alpha8(alpha)),
            width * self.scale,
        ));
    }

    /// 柔光点：三层同心半透明圆近似径向渐变。
    pub(super) fn glow(&mut self, c: V, radius: f32, color: u32, alpha: f32) {
        for (scale, weight) in [(1.0, 0.18), (0.62, 0.32), (0.3, 0.5)] {
            self.dot(c, radius * scale, color, alpha * weight);
        }
    }

    /// 卡片圆角描边；`expand` 向外扩，`dx` 水平错位（色散用）。
    pub(super) fn card_outline(
        &mut self,
        card: Rect,
        expand: f32,
        width: f32,
        color: u32,
        alpha: f32,
        dx: f32,
    ) {
        if alpha <= 0.0 {
            return;
        }
        let e = expand * self.scale;
        let bounds = Bounds {
            origin: point(px(card.x - e + dx * self.scale), px(card.y - e)),
            size: gpui::Size {
                width: px(card.w + e * 2.0),
                height: px(card.h + e * 2.0),
            },
        };
        self.plan.quads.push(outline_quad(
            bounds,
            14.0 + e,
            with_alpha(color, alpha8(alpha)),
            width * self.scale,
        ));
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/dispatch_style_painter_tests.rs"]
mod tests;
