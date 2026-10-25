//! Agent 卡片运行特效（非经典风格）的 GPUI 绘制：把 `motion::run_style` 的图元转成路径与 quad。

mod view;

pub(crate) use view::{RunEffectPresentation, RunEffectView};

use gpui::{
    fill, linear_color_stop, linear_gradient, point, px, rgba, size, Background, Bounds,
    PathBuilder, Pixels, Point, Window,
};

use crate::motion::run_style::{RunFill, RunPrim};

/// 在给定原点下绘制图元。返回实际提交的绘制数，供诊断。
pub(crate) fn paint_prims(prims: &[RunPrim], origin: Point<Pixels>, window: &mut Window) -> usize {
    let at = |p: &[f32; 2]| point(origin.x + px(p[0]), origin.y + px(p[1]));
    let mut painted = 0;
    for prim in prims {
        match prim {
            RunPrim::Stroke {
                points,
                closed,
                width,
                color,
            } => {
                let mut builder = PathBuilder::stroke(px(*width));
                builder.add_polygon(&points.iter().map(at).collect::<Vec<_>>(), *closed);
                if let Ok(path) = builder.build() {
                    window.paint_path(path, rgba(*color));
                    painted += 1;
                }
            }
            RunPrim::Fill { contours, fill } => {
                // 默认奇偶填充：外轮廓 + 卡片轮廓即一圈环形膜。
                let mut builder = PathBuilder::fill();
                for contour in contours {
                    builder.add_polygon(&contour.iter().map(at).collect::<Vec<_>>(), true);
                }
                let background: Background = match *fill {
                    RunFill::Solid(color) => rgba(color).into(),
                    RunFill::Vertical { top, bottom } => linear_gradient(
                        180.0,
                        linear_color_stop(rgba(top), 0.0),
                        linear_color_stop(rgba(bottom), 1.0),
                    ),
                };
                if let Ok(path) = builder.build() {
                    window.paint_path(path, background);
                    painted += 1;
                }
            }
            RunPrim::Bar {
                x0,
                x1,
                y,
                height,
                left,
                right,
            } => {
                if x1 <= x0 {
                    continue;
                }
                let bounds = Bounds {
                    origin: at(&[*x0, *y]),
                    size: size(px(x1 - x0), px(*height)),
                };
                let background: Background = if left == right {
                    rgba(*left).into()
                } else {
                    linear_gradient(
                        90.0,
                        linear_color_stop(rgba(*left), 0.0),
                        linear_color_stop(rgba(*right), 1.0),
                    )
                };
                window.paint_quad(fill(bounds, background));
                painted += 1;
            }
        }
    }
    painted
}
