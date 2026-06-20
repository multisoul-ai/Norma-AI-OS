//! 几何与坐标变换。约定：screen = canvas * zoom + pan。

use std::ops::{Add, Sub};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };
    pub fn new(x: f32, y: f32) -> Self {
        Vec2 { x, y }
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, rhs: Vec2) -> Vec2 {
        Vec2::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, rhs: Vec2) -> Vec2 {
        Vec2::new(self.x - rhs.x, self.y - rhs.y)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasRect {
    pub origin: Vec2,
    pub size: Vec2,
}

impl CanvasRect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        CanvasRect { origin: Vec2::new(x, y), size: Vec2::new(w, h) }
    }

    /// 右下角坐标（origin + size）。
    pub fn max(&self) -> Vec2 {
        self.origin + self.size
    }

    /// 半开区间命中：x∈[origin.x, max.x), y∈[origin.y, max.y)。
    pub fn contains(&self, p: Vec2) -> bool {
        let max = self.max();
        p.x >= self.origin.x && p.x < max.x && p.y >= self.origin.y && p.y < max.y
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform2D {
    pub scale: f32,
    pub translate: Vec2,
}

impl Transform2D {
    pub fn apply(&self, p: Vec2) -> Vec2 {
        Vec2::new(p.x * self.scale + self.translate.x, p.y * self.scale + self.translate.y)
    }

    pub fn invert(&self, p: Vec2) -> Vec2 {
        Vec2::new((p.x - self.translate.x) / self.scale, (p.y - self.translate.y) / self.scale)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub pan: Vec2,
    pub zoom: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Viewport { pan: Vec2::ZERO, zoom: 1.0 }
    }
}

impl Viewport {
    pub fn transform(&self) -> Transform2D {
        Transform2D { scale: self.zoom, translate: self.pan }
    }

    pub fn canvas_to_screen(&self, p: Vec2) -> Vec2 {
        self.transform().apply(p)
    }

    pub fn screen_to_canvas(&self, p: Vec2) -> Vec2 {
        self.transform().invert(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// f32 近似比较（坐标变换含乘除，需容差）。eps=1e-4 足够覆盖本切片像素级精度。
    fn approx(a: Vec2, b: Vec2) -> bool {
        (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4
    }

    /// 坐标换算：pan + zoom 下 canvas↔screen 双向变换正确且可逆。
    ///
    /// 数据构造（数值推导）：
    ///   viewport.pan  = (100, 50)   （画布被平移）
    ///   viewport.zoom = 2.0         （放大一倍）
    ///   canvas 点 P   = (10, 20)
    ///
    /// 执行过程（screen = canvas*zoom + pan）：
    ///   1. canvas_to_screen(P) = (10*2+100, 20*2+50) = (120, 90)
    ///   2. screen_to_canvas((120,90)) = ((120-100)/2, (90-50)/2) = (10, 20)
    ///
    /// 预期结果：
    ///   - 正断言：canvas_to_screen(P) == (120, 90)
    ///   - 正断言：回程 screen_to_canvas 还原为原始 P (10,20)
    ///   - 负断言：screen 坐标 (120,90) 不等于 canvas 坐标 (10,20)（证明确实发生了变换，而非恒等返回）
    #[test]
    fn test_canvas_screen_roundtrip_under_pan_zoom() {
        let vp = Viewport { pan: Vec2::new(100.0, 50.0), zoom: 2.0 };
        let p = Vec2::new(10.0, 20.0);

        let screen = vp.canvas_to_screen(p);
        assert!(
            approx(screen, Vec2::new(120.0, 90.0)),
            "canvas_to_screen 应为 (120,90)，实际 {:?}：变换公式 screen=canvas*zoom+pan 出错",
            screen
        );

        let back = vp.screen_to_canvas(screen);
        assert!(
            approx(back, p),
            "screen_to_canvas 回程应还原为 (10,20)，实际 {:?}：逆变换 (screen-pan)/zoom 出错",
            back
        );

        assert!(
            !approx(screen, p),
            "screen 与 canvas 坐标不应相等，否则说明变换是恒等的、pan/zoom 未生效"
        );
    }

    /// 默认 Viewport（pan=0, zoom=1）下 canvas 与 screen 重合（恒等变换）。
    ///
    /// 预期结果：
    ///   - 正断言：任意点经默认 viewport 变换后不变
    #[test]
    fn test_default_viewport_is_identity() {
        let vp = Viewport::default();
        let p = Vec2::new(7.0, -3.0);
        assert!(
            approx(vp.canvas_to_screen(p), p),
            "默认 viewport 应为恒等变换，pan=0/zoom=1 下点不应移动"
        );
    }

    /// 矩形命中：点在矩形内返回 true，在外返回 false（含正/负断言）。
    ///
    /// 数据构造：
    ///   rect = origin(10,10) size(100x50) → 覆盖 x∈[10,110), y∈[10,60)
    ///   inside  = (50, 30)   （明显在内）
    ///   outside = (200, 30)  （x 超出右边界）
    ///
    /// 预期结果：
    ///   - 正断言：contains(inside) == true
    ///   - 负断言：contains(outside) == false
    #[test]
    fn test_rect_contains_inside_and_outside() {
        let rect = CanvasRect::new(10.0, 10.0, 100.0, 50.0);
        assert!(
            rect.contains(Vec2::new(50.0, 30.0)),
            "(50,30) 落在 rect[10,110)x[10,60) 内，contains 应为 true"
        );
        assert!(
            !rect.contains(Vec2::new(200.0, 30.0)),
            "(200,30) x 超出右边界 110，contains 应为 false"
        );
    }
}
