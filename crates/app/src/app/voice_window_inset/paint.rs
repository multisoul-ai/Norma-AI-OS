//! 把风格输出的图元画到 GPUI：这里是唯一调用绘制 API 的地方。
//! 设置页预览复用 `paint_prims`，用 `scale` 把整窗坐标缩进缩略图。

use std::sync::Arc;

use gpui::{
    canvas, div, point, prelude::*, px, size, Bounds, BoxShadow, Corners, PathBuilder, Pixels,
    Point, Stateful, Window,
};

use crate::effects::voice_glow::scene::GlowPrim;

pub(crate) fn paint_prims(
    window: &mut Window,
    origin: Point<Pixels>,
    scale: f32,
    prims: &[GlowPrim],
) {
    let at = |x: f32, y: f32| point(origin.x + px(x * scale), origin.y + px(y * scale));
    for prim in prims {
        match prim {
            GlowPrim::Glow {
                rect,
                radius,
                blur,
                rgba,
            } => window.paint_shadows(
                Bounds {
                    origin: at(rect.x, rect.y),
                    size: size(px(rect.w * scale), px(rect.h * scale)),
                },
                Corners::all(px(radius * scale)),
                &[BoxShadow {
                    color: gpui::rgba(*rgba).into(),
                    offset: point(px(0.0), px(0.0)),
                    blur_radius: px(blur * scale),
                    spread_radius: px(0.0),
                }],
            ),
            GlowPrim::Fill { pts, rgba } => {
                let mut builder = PathBuilder::fill();
                builder.add_polygon(&pts.iter().map(|p| at(p.0, p.1)).collect::<Vec<_>>(), true);
                if let Ok(path) = builder.build() {
                    window.paint_path(path, gpui::rgba(*rgba));
                }
            }
            GlowPrim::Stroke { pts, width, rgba } => {
                let mut builder = PathBuilder::stroke(px((width * scale).max(0.5)));
                builder.add_polygon(&pts.iter().map(|p| at(p.0, p.1)).collect::<Vec<_>>(), false);
                if let Ok(path) = builder.build() {
                    window.paint_path(path, gpui::rgba(*rgba));
                }
            }
        }
    }
}

/// 覆盖整窗、不接收鼠标事件的光效层。
pub(crate) fn render(prims: Arc<Vec<GlowPrim>>) -> Stateful<gpui::Div> {
    div()
        .id("voice-window-inset")
        .debug_selector(|| "voice-window-inset".to_string())
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| paint_prims(window, bounds.origin, 1.0, &prims),
            )
            .absolute()
            .size_full(),
        )
}
