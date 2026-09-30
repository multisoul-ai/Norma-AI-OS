//! Agent 完成提示的外缘柔光：透明填充，三层圆角环只占卡片边界以外。

use canvas::CanvasRect;
use gpui::{
    canvas, point, prelude::*, px, quad, rgba, size, AnyElement, Bounds, PaintQuad, Pixels,
};

use crate::motion::visual::{BorderGlow, ResolvedNodeVisual};

const MARGIN_PX: f32 = 12.0;

#[derive(Clone, Copy, Debug)]
struct GlowBand {
    inner: f32,
    outer: f32,
    alpha: u8,
}

pub(crate) fn render_border_glow(
    rect: CanvasRect,
    radius: f32,
    zoom: f32,
    visual: ResolvedNodeVisual,
) -> Option<AnyElement> {
    let glow = visual.border_glow?;
    let (x, y, width, height) = (rect.origin.x, rect.origin.y, rect.size.x, rect.size.y);
    let bands = glow_bands(glow, zoom, visual.opacity);
    let color = visual.stroke_color;
    Some(
        canvas(
            move |bounds, _window, _cx| {
                let card = Bounds {
                    origin: point(bounds.left() + px(MARGIN_PX), bounds.top() + px(MARGIN_PX)),
                    size: size(px(width), px(height)),
                };
                bands.map(|band| glow_quad(card, radius, color, band))
            },
            move |_bounds, quads, window, _cx| {
                for quad in quads {
                    window.paint_quad(quad);
                }
            },
        )
        .absolute()
        .left(px(x - MARGIN_PX))
        .top(px(y - MARGIN_PX))
        .w(px(width + MARGIN_PX * 2.0))
        .h(px(height + MARGIN_PX * 2.0))
        .into_any_element(),
    )
}

fn glow_bands(glow: BorderGlow, zoom: f32, opacity: f32) -> [GlowBand; 3] {
    let flash = glow.flash.clamp(0.0, 1.0);
    let strength = (glow.intensity + flash * 0.3).clamp(0.0, 1.0) * opacity.clamp(0.0, 1.0);
    let extent = (8.0 * zoom).clamp(5.0, 9.0) + flash * 3.0;
    let inner = extent * 0.28;
    let middle = extent * 0.64;
    [
        GlowBand {
            inner: middle,
            outer: extent,
            alpha: ((0.10 + flash * 0.08) * strength * 255.0).round() as u8,
        },
        GlowBand {
            inner,
            outer: middle,
            alpha: ((0.22 + flash * 0.10) * strength * 255.0).round() as u8,
        },
        GlowBand {
            inner: 0.0,
            outer: inner,
            alpha: ((0.42 + flash * 0.18) * strength * 255.0).round() as u8,
        },
    ]
}

fn glow_quad(card: Bounds<Pixels>, radius: f32, color: u32, band: GlowBand) -> PaintQuad {
    let outer = band.outer;
    let expanded = Bounds {
        origin: point(card.origin.x - px(outer), card.origin.y - px(outer)),
        size: size(
            card.size.width + px(outer * 2.0),
            card.size.height + px(outer * 2.0),
        ),
    };
    quad(
        expanded,
        px(radius + outer),
        rgba(0x00000000),
        px(outer - band.inner),
        rgba((color & 0xffffff00) | u32::from(band.alpha)),
        Default::default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// BG-1: 三层柔光均在卡片外缘，完成瞬间外扩且更亮。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   zoom=1 → 常态外缘 8px；flash=1 → 8+3=11px，小于 12px 画布留白。
    ///   intensity=1、opacity=1 → alpha 不受整体透明度衰减。
    ///
    /// 执行过程：
    ///   1. 计算常态三层环与 flash=1 的三层环。
    ///   2. 检查每层内外半径，以及亮起前后的范围和透明度。
    ///
    /// 预期结果：
    ///   - 正断言：三层从外到内连续衔接；亮起时范围、透明度增加。
    ///   - 负断言：任何环都不能进入卡片内部或超出 12px 留白。
    #[test]
    fn test_glow_bands_stay_outside_card_and_flash_once() {
        let steady = glow_bands(
            BorderGlow {
                intensity: 1.0,
                flash: 0.0,
            },
            1.0,
            1.0,
        );
        let flash = glow_bands(
            BorderGlow {
                intensity: 1.0,
                flash: 1.0,
            },
            1.0,
            1.0,
        );

        assert_eq!(steady[2].inner, 0.0, "最内层必须从卡片外边界开始");
        assert_eq!(steady[1].inner, steady[2].outer, "内层与中层应连续");
        assert_eq!(steady[0].inner, steady[1].outer, "中层与外层应连续");
        assert!(steady[0].outer <= MARGIN_PX, "常态柔光不能超出画布留白");
        assert!(flash[0].outer <= MARGIN_PX, "亮起柔光不能超出画布留白");
        assert!(flash[0].outer > steady[0].outer, "亮起时外缘应短暂扩张");
        assert!(flash[2].alpha > steady[2].alpha, "亮起时内层应更明亮");
        assert!(steady[0].inner >= 0.0, "外层不能进入卡片内部");
        assert!(steady[1].inner >= 0.0, "中层不能进入卡片内部");
        assert!(steady[2].inner >= 0.0, "内层不能进入卡片内部");
    }
}
