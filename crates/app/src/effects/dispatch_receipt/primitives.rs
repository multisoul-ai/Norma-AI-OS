use crate::motion::DispatchReceiptVisualStatus;
use gpui::{
    point, px, quad, rgba, Bounds, Edges, PaintQuad, Path, PathBuilder, Pixels, Point, Size,
};
use ui::Theme;

pub(super) fn status_style(status: DispatchReceiptVisualStatus, theme: Theme) -> (u32, f32) {
    let intensity = impact_radius_scale(status);
    let color = match status {
        DispatchReceiptVisualStatus::Submitted => theme.accent_cyan,
        DispatchReceiptVisualStatus::Queued => theme.accent_blue,
        DispatchReceiptVisualStatus::Untracked => theme.text_secondary,
    };
    (color, intensity)
}

pub(crate) fn impact_radius_scale(status: DispatchReceiptVisualStatus) -> f32 {
    match status {
        DispatchReceiptVisualStatus::Submitted => 1.0,
        DispatchReceiptVisualStatus::Queued => 0.72,
        DispatchReceiptVisualStatus::Untracked => 0.78,
    }
}

pub(super) fn centered_bounds(center: Point<Pixels>, width: f32, height: f32) -> Bounds<Pixels> {
    Bounds {
        origin: point(center.x - px(width / 2.0), center.y - px(height / 2.0)),
        size: Size {
            width: px(width),
            height: px(height),
        },
    }
}

pub(super) fn expanded_bounds(bounds: Bounds<Pixels>, amount: f32) -> Bounds<Pixels> {
    Bounds {
        origin: point(bounds.origin.x - px(amount), bounds.origin.y - px(amount)),
        size: Size {
            width: bounds.size.width + px(amount * 2.0),
            height: bounds.size.height + px(amount * 2.0),
        },
    }
}

pub(super) fn round_quad(bounds: Bounds<Pixels>, radius: f32, color: u32) -> PaintQuad {
    quad(
        bounds,
        px(radius),
        rgba(color),
        Edges::<Pixels>::default(),
        rgba(0x00000000),
        Default::default(),
    )
}

pub(super) fn outline_quad(
    bounds: Bounds<Pixels>,
    radius: f32,
    color: u32,
    width: f32,
) -> PaintQuad {
    quad(
        bounds,
        px(radius),
        rgba(0x00000000),
        px(width),
        rgba(color),
        Default::default(),
    )
}

pub(super) fn oriented_ellipse_path(
    center: Point<Pixels>,
    tangent: (f32, f32),
    major: f32,
    minor: f32,
    stroke_width: f32,
) -> Option<Path<Pixels>> {
    let normal = (-tangent.1, tangent.0);
    let mut builder = PathBuilder::stroke(px(stroke_width));
    for index in 0..=24 {
        let angle = std::f32::consts::TAU * index as f32 / 24.0;
        let along = angle.cos() * major;
        let across = angle.sin() * minor;
        let sample = point(
            center.x + px(tangent.0 * along + normal.0 * across),
            center.y + px(tangent.1 * along + normal.1 * across),
        );
        if index == 0 {
            builder.move_to(sample);
        } else {
            builder.line_to(sample);
        }
    }
    builder.build().ok()
}

pub(super) fn ease_out_cubic(progress: f32) -> f32 {
    1.0 - (1.0 - progress.clamp(0.0, 1.0)).powi(3)
}

pub(super) fn smoothstep(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    progress * progress * (3.0 - 2.0 * progress)
}

pub(super) fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
