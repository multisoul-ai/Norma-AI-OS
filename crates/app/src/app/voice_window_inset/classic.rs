//! 经典风格：窗口四边向内的白色线性渐变（原「说话光晕」），逻辑自 `voice_window_inset.rs` 原样迁入。

use super::{seam, VoiceWindowInsetVisual, EPSILON};
use gpui::{
    canvas, div, linear_color_stop, linear_gradient, point, prelude::*, px, rgba, Background,
    Bounds, Path, PathBuilder, Pixels, Stateful,
};

pub(crate) fn render(visual: VoiceWindowInsetVisual) -> Stateful<gpui::Div> {
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
                move |bounds, window, _cx| {
                    build_unified_inset_paths(bounds, visual, window.scale_factor())
                },
                move |_bounds, paths, window, _cx| {
                    for (path, background) in paths {
                        window.paint_path(path, background);
                    }
                },
            )
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .size_full(),
        )
}

#[derive(Clone, Copy, Debug)]
enum InsetGradient {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug)]
enum InsetRegion {
    Rect {
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
        gradient: InsetGradient,
    },
    Triangle {
        a: (f32, f32),
        b: (f32, f32),
        c: (f32, f32),
        gradient: InsetGradient,
    },
}

fn build_unified_inset_paths(
    bounds: Bounds<Pixels>,
    visual: VoiceWindowInsetVisual,
    scale_factor: f32,
) -> Vec<(Path<Pixels>, Background)> {
    let alpha = (visual.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    let white = rgba(0xffffff00 | u32::from(alpha));
    let transparent = rgba(0xffffff00);
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);

    let mut paths = inset_partition(width, height, visual.inset_px)
        .into_iter()
        .filter_map(|region| {
            region
                .path(bounds.origin)
                .map(|path| (path, region.gradient().background(white, transparent)))
        })
        .collect::<Vec<_>>();
    paths.extend(seam::build_corner_seam_paths(
        bounds.origin,
        width,
        height,
        visual.inset_px,
        visual.opacity,
        scale_factor,
    ));
    paths
}

fn inset_partition(width: f32, height: f32, requested_inset: f32) -> Vec<InsetRegion> {
    let width = width.max(0.0);
    let height = height.max(0.0);
    let inset = requested_inset.clamp(0.0, width.min(height) * 0.5);
    if inset <= EPSILON {
        return Vec::new();
    }

    let right = width - inset;
    let bottom = height - inset;
    let mut regions = Vec::with_capacity(12);

    if right - inset > EPSILON {
        regions.push(InsetRegion::Rect {
            left: inset,
            top: 0.0,
            right,
            bottom: inset,
            gradient: InsetGradient::Top,
        });
        regions.push(InsetRegion::Rect {
            left: inset,
            top: bottom,
            right,
            bottom: height,
            gradient: InsetGradient::Bottom,
        });
    }
    if bottom - inset > EPSILON {
        regions.push(InsetRegion::Rect {
            left: 0.0,
            top: inset,
            right: inset,
            bottom,
            gradient: InsetGradient::Left,
        });
        regions.push(InsetRegion::Rect {
            left: right,
            top: inset,
            right: width,
            bottom,
            gradient: InsetGradient::Right,
        });
    }

    regions.extend([
        InsetRegion::Triangle {
            a: (0.0, 0.0),
            b: (inset, 0.0),
            c: (inset, inset),
            gradient: InsetGradient::Top,
        },
        InsetRegion::Triangle {
            a: (0.0, 0.0),
            b: (inset, inset),
            c: (0.0, inset),
            gradient: InsetGradient::Left,
        },
        InsetRegion::Triangle {
            a: (right, 0.0),
            b: (width, 0.0),
            c: (right, inset),
            gradient: InsetGradient::Top,
        },
        InsetRegion::Triangle {
            a: (width, 0.0),
            b: (width, inset),
            c: (right, inset),
            gradient: InsetGradient::Right,
        },
        InsetRegion::Triangle {
            a: (right, height),
            b: (right, bottom),
            c: (width, height),
            gradient: InsetGradient::Bottom,
        },
        InsetRegion::Triangle {
            a: (width, height),
            b: (right, bottom),
            c: (width, bottom),
            gradient: InsetGradient::Right,
        },
        InsetRegion::Triangle {
            a: (0.0, height),
            b: (inset, bottom),
            c: (inset, height),
            gradient: InsetGradient::Bottom,
        },
        InsetRegion::Triangle {
            a: (0.0, height),
            b: (0.0, bottom),
            c: (inset, bottom),
            gradient: InsetGradient::Left,
        },
    ]);
    regions
}

impl InsetGradient {
    fn background(self, white: gpui::Rgba, transparent: gpui::Rgba) -> Background {
        let angle = match self {
            Self::Top => 180.0,
            Self::Bottom => 0.0,
            Self::Left => 90.0,
            Self::Right => 270.0,
        };
        linear_gradient(
            angle,
            linear_color_stop(white, 0.0),
            linear_color_stop(transparent, 1.0),
        )
    }
}

impl InsetRegion {
    fn gradient(self) -> InsetGradient {
        match self {
            Self::Rect { gradient, .. } | Self::Triangle { gradient, .. } => gradient,
        }
    }

    fn path(self, origin: gpui::Point<Pixels>) -> Option<Path<Pixels>> {
        let offset_x = f32::from(origin.x);
        let offset_y = f32::from(origin.y);
        let to_point = |(x, y): (f32, f32)| point(px(offset_x + x), px(offset_y + y));
        let mut builder = PathBuilder::fill();
        match self {
            Self::Rect {
                left,
                top,
                right,
                bottom,
                ..
            } => builder.add_polygon(
                &[
                    to_point((left, top)),
                    to_point((right, top)),
                    to_point((right, bottom)),
                    to_point((left, bottom)),
                ],
                true,
            ),
            Self::Triangle { a, b, c, .. } => {
                builder.add_polygon(&[to_point(a), to_point(b), to_point(c)], true)
            }
        }
        builder.build().ok()
    }

    #[cfg(test)]
    fn contains(self, x: f32, y: f32) -> bool {
        match self {
            Self::Rect {
                left,
                top,
                right,
                bottom,
                ..
            } => x >= left && x < right && y >= top && y < bottom,
            Self::Triangle { a, b, c, .. } => {
                let cross = |from: (f32, f32), to: (f32, f32)| {
                    (to.0 - from.0) * (y - from.1) - (to.1 - from.1) * (x - from.0)
                };
                let ab = cross(a, b);
                let bc = cross(b, c);
                let ca = cross(c, a);
                (ab >= 0.0 && bc >= 0.0 && ca >= 0.0) || (ab <= 0.0 && bc <= 0.0 && ca <= 0.0)
            }
        }
    }
}

#[cfg(test)]
pub(super) fn inset_partition_coverage(
    width: f32,
    height: f32,
    inset: f32,
    x: f32,
    y: f32,
) -> usize {
    inset_partition(width, height, inset)
        .into_iter()
        .filter(|region| region.contains(x, y))
        .count()
}
