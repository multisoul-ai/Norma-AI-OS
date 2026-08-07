//! 角部分区共享斜边的抗锯齿透明度补偿。

use super::{CORNER_SEAM_DEVICE_PX, EPSILON};
use gpui::{
    linear_color_stop, linear_gradient, point, px, rgba, Background, Path, PathBuilder, Pixels,
};

pub(super) fn build_corner_seam_paths(
    origin: gpui::Point<Pixels>,
    width: f32,
    height: f32,
    requested_inset: f32,
    edge_opacity: f32,
    scale_factor: f32,
) -> Vec<(Path<Pixels>, Background)> {
    let inset = requested_inset.clamp(0.0, width.min(height) * 0.5);
    let compensation = corner_seam_compensation_alpha(edge_opacity);
    if inset <= EPSILON || compensation <= EPSILON {
        return Vec::new();
    }

    let right = width - inset;
    let bottom = height - inset;
    let seam_width = CORNER_SEAM_DEVICE_PX / scale_factor.max(1.0);
    let offset_x = f32::from(origin.x);
    let offset_y = f32::from(origin.y);
    let to_point = |(x, y): (f32, f32)| point(px(offset_x + x), px(offset_y + y));
    let white = rgba(0xffffff00 | (compensation * 255.0).round() as u32);
    let transparent = rgba(0xffffff00);
    let seams = [
        ((0.0, 0.0), (inset, inset), 135.0),
        ((width, 0.0), (right, inset), 225.0),
        ((width, height), (right, bottom), 315.0),
        ((0.0, height), (inset, bottom), 45.0),
    ];

    seams
        .into_iter()
        .filter_map(|(from, to, angle)| {
            let mut builder = PathBuilder::stroke(px(seam_width));
            builder.move_to(to_point(from));
            builder.line_to(to_point(to));
            builder.build().ok().map(|path| {
                (
                    path,
                    linear_gradient(
                        angle,
                        linear_color_stop(white, 0.0),
                        linear_color_stop(transparent, 1.0),
                    ),
                )
            })
        })
        .collect()
}

pub(super) fn corner_seam_compensation_alpha(edge_alpha: f32) -> f32 {
    let edge_alpha = edge_alpha.clamp(0.0, 1.0);
    let half_coverage_alpha = edge_alpha * 0.5;
    let split_corner_alpha =
        half_coverage_alpha + half_coverage_alpha * (1.0 - half_coverage_alpha);
    ((edge_alpha - split_corner_alpha) / (1.0 - split_corner_alpha).max(EPSILON)).clamp(0.0, 1.0)
}
