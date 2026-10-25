//! 卡片几何：圆角矩形周长上的点与外法线，和原型的 `perimRR()` 一致（从上边左端顺时针）。

/// 卡片在绘制区域内的位置（像素，已乘缩放）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CardFrame {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub radius: f32,
    pub zoom: f32,
    /// 标题栏高度（像素），一道光画在它的下边线上。
    pub title_h: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EdgePoint {
    pub x: f32,
    pub y: f32,
    pub nx: f32,
    pub ny: f32,
}

impl CardFrame {
    fn radius_clamped(&self) -> f32 {
        self.radius.clamp(0.0, self.w.min(self.h).max(0.0) * 0.5)
    }

    pub(crate) fn perimeter(&self) -> f32 {
        let r = self.radius_clamped();
        2.0 * (self.w - 2.0 * r).max(0.0)
            + 2.0 * (self.h - 2.0 * r).max(0.0)
            + std::f32::consts::TAU * r
    }

    /// `u` 为周长比例，任意实数按 1 取模。
    pub(crate) fn edge(&self, u: f32) -> EdgePoint {
        let r = self.radius_clamped();
        let sx = (self.w - 2.0 * r).max(0.0);
        let sy = (self.h - 2.0 * r).max(0.0);
        let quarter = std::f32::consts::FRAC_PI_2 * r;
        let mut s = u.rem_euclid(1.0) * self.perimeter();
        let (x0, y0, x1, y1) = (self.x, self.y, self.x + self.w, self.y + self.h);
        let arc = |cx: f32, cy: f32, start: f32, s: f32| {
            let a = start + if r > 0.0 { s / r } else { 0.0 };
            let (sin, cos) = a.sin_cos();
            EdgePoint {
                x: cx + r * cos,
                y: cy + r * sin,
                nx: cos,
                ny: sin,
            }
        };
        let line = |x: f32, y: f32, nx: f32, ny: f32| EdgePoint { x, y, nx, ny };
        use std::f32::consts::{FRAC_PI_2, PI};
        let pieces: [(f32, &dyn Fn(f32) -> EdgePoint); 8] = [
            (sx, &|s| line(x0 + r + s, y0, 0.0, -1.0)),
            (quarter, &|s| arc(x1 - r, y0 + r, -FRAC_PI_2, s)),
            (sy, &|s| line(x1, y0 + r + s, 1.0, 0.0)),
            (quarter, &|s| arc(x1 - r, y1 - r, 0.0, s)),
            (sx, &|s| line(x1 - r - s, y1, 0.0, 1.0)),
            (quarter, &|s| arc(x0 + r, y1 - r, FRAC_PI_2, s)),
            (sy, &|s| line(x0, y1 - r - s, -1.0, 0.0)),
            (quarter, &|s| arc(x0 + r, y0 + r, PI, s)),
        ];
        for (length, at) in pieces {
            if s <= length {
                return at(s);
            }
            s -= length;
        }
        line(x0 + r, y0, 0.0, -1.0)
    }

    /// 底边中点在周长上的比例：磁流体等待审批的液滴落在这里。
    pub(crate) fn bottom_center_u(&self) -> f32 {
        let r = self.radius_clamped();
        let sx = (self.w - 2.0 * r).max(0.0);
        let sy = (self.h - 2.0 * r).max(0.0);
        let quarter = std::f32::consts::FRAC_PI_2 * r;
        let perimeter = self.perimeter();
        if perimeter <= 0.0 {
            return 0.5;
        }
        (sx + 2.0 * quarter + sy + sx * 0.5) / perimeter
    }
}

/// 周长比例的环形距离，结果在 [0, 0.5]。
pub(crate) fn ring_distance(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(1.0);
    d.min(1.0 - d)
}

pub(crate) fn gaussian(x: f32, sigma: f32) -> f32 {
    (-(x / sigma).powi(2)).exp()
}

/// 心跳：每 1.2s 一次双搏，与原型的 `beat()` 一致，返回 0..≈1。
pub(crate) fn heartbeat(time_ms: f32) -> f32 {
    let period = 1200.0;
    let m = time_ms.rem_euclid(period);
    gaussian(m, 55.0) + gaussian(m - period, 55.0) + 0.65 * gaussian(m - 230.0, 60.0)
}

/// 确定性伪随机，返回 [0,1)。
pub(crate) fn hash(n: f32) -> f32 {
    let v = (n * 127.1 + 311.7).sin() * 43_758.547;
    v - v.floor()
}

pub(crate) use crate::motion::util::ease_out_cubic;
