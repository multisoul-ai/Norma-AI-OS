//! Rounded-path working-state border segment for Agent cards.

mod view;

pub(crate) use view::WorkingSnakeView;

use gpui::{
    canvas, linear_color_stop, linear_gradient, point, prelude::*, px, rgba, Background, Bounds,
    Path, PathBuilder, Pixels,
};

use canvas::NodeId;

use crate::motion::working::WorkingSnake;

const EPSILON: f32 = 0.001;

pub(crate) fn render_working_snake_border(
    node_id: NodeId,
    snake: WorkingSnake,
    zoom: f32,
    node_radius_px: f32,
) -> impl IntoElement {
    canvas(
        move |bounds, _window, _cx| {
            let build_started_at = std::time::Instant::now();
            let paths = build_snake_paths(bounds, snake, zoom, node_radius_px);
            crate::frame_diagnostics::SnakePaintReport {
                node_id,
                width_px: f32::from(bounds.right()) - f32::from(bounds.left()),
                height_px: f32::from(bounds.bottom()) - f32::from(bounds.top()),
                zoom,
                path_count: paths.len(),
                elapsed: build_started_at.elapsed(),
            }
            .log_if_enabled();
            paths
        },
        move |_bounds, paths, window, _cx| {
            for (path, color) in paths {
                window.paint_path(path, color);
            }
        },
    )
    .absolute()
    .top(px(0.0))
    .left(px(0.0))
    .right(px(0.0))
    .bottom(px(0.0))
}

fn build_snake_paths(
    bounds: Bounds<Pixels>,
    snake: WorkingSnake,
    zoom: f32,
    node_radius_px: f32,
) -> Vec<(Path<Pixels>, Background)> {
    let stroke_width = snake.stroke_px * zoom;
    let centerline_inset = (snake.inset_px * zoom).max(stroke_width * 0.5 + 1.0);
    let track = RoundedTrack::new(bounds, centerline_inset, node_radius_px - centerline_inset);
    let ranges = snake_ranges(snake.head_phase, snake.tail_fraction);
    let mut paths = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        if let Some(segment) = build_track_segment(&track, start, end, stroke_width) {
            paths.push((
                segment,
                linear_gradient(
                    snake.head_phase * 360.0,
                    linear_color_stop(rgba(snake.tail_color), 0.0),
                    linear_color_stop(rgba(snake.head_color), 1.0),
                ),
            ));
        }
    }
    paths
}

fn snake_ranges(head_phase: f32, tail_fraction: f32) -> Vec<(f32, f32)> {
    let head = head_phase.rem_euclid(1.0);
    let tail = (head - tail_fraction.clamp(0.02, 0.48)).rem_euclid(1.0);
    if tail <= head {
        vec![(tail, head)]
    } else {
        vec![(tail, 1.0), (0.0, head)]
    }
}

fn build_track_segment(
    track: &RoundedTrack,
    start_phase: f32,
    end_phase: f32,
    stroke_width: f32,
) -> Option<Path<Pixels>> {
    if end_phase - start_phase <= EPSILON || track.perimeter <= EPSILON || stroke_width <= EPSILON {
        return None;
    }

    let mut builder = PathBuilder::stroke(px(stroke_width));
    let start_distance = start_phase * track.perimeter;
    let end_distance = end_phase * track.perimeter;
    let mut has_point = false;

    for component in track.components() {
        let component_start = component.offset;
        let component_end = component.offset + component.length;
        let overlap_start = start_distance.max(component_start);
        let overlap_end = end_distance.min(component_end);
        if overlap_end - overlap_start <= EPSILON {
            continue;
        }

        let local_start = (overlap_start - component_start) / component.length;
        let local_end = (overlap_end - component_start) / component.length;
        let from = component.point_at(local_start);
        let to = component.point_at(local_end);
        if !has_point {
            builder.move_to(from);
            has_point = true;
        }

        match component.kind {
            TrackComponentKind::Line { .. } => builder.line_to(to),
            TrackComponentKind::Arc { radius, .. } => {
                builder.arc_to(point(px(radius), px(radius)), px(0.0), false, true, to);
            }
        }
    }

    has_point.then(|| builder.build().ok()).flatten()
}

#[derive(Clone, Copy, Debug)]
struct RoundedTrack {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    radius: f32,
    perimeter: f32,
}

impl RoundedTrack {
    fn new(bounds: Bounds<Pixels>, inset: f32, radius: f32) -> Self {
        let left = f32::from(bounds.left()) + inset;
        let top = f32::from(bounds.top()) + inset;
        let right = f32::from(bounds.right()) - inset;
        let bottom = f32::from(bounds.bottom()) - inset;
        let width = (right - left).max(1.0);
        let height = (bottom - top).max(1.0);
        let radius = radius.clamp(0.0, width.min(height) * 0.5);
        let straight_x = (width - radius * 2.0).max(0.0);
        let straight_y = (height - radius * 2.0).max(0.0);
        let perimeter = straight_x * 2.0 + straight_y * 2.0 + std::f32::consts::TAU * radius;
        Self {
            left,
            top,
            right,
            bottom,
            radius,
            perimeter,
        }
    }

    fn components(&self) -> [TrackComponent; 8] {
        let r = self.radius;
        let top_len = (self.right - self.left - 2.0 * r).max(0.0);
        let side_len = (self.bottom - self.top - 2.0 * r).max(0.0);
        let arc_len = std::f32::consts::FRAC_PI_2 * r;
        let mut offset = 0.0;
        let mut next = |length: f32, kind: TrackComponentKind| {
            let component = TrackComponent {
                offset,
                length: length.max(EPSILON),
                kind,
            };
            offset += length;
            component
        };

        [
            next(
                top_len,
                TrackComponentKind::Line {
                    from: point(px(self.left + r), px(self.top)),
                    to: point(px(self.right - r), px(self.top)),
                },
            ),
            next(
                arc_len,
                TrackComponentKind::Arc {
                    center: point(px(self.right - r), px(self.top + r)),
                    radius: r,
                    start_angle: -std::f32::consts::FRAC_PI_2,
                },
            ),
            next(
                side_len,
                TrackComponentKind::Line {
                    from: point(px(self.right), px(self.top + r)),
                    to: point(px(self.right), px(self.bottom - r)),
                },
            ),
            next(
                arc_len,
                TrackComponentKind::Arc {
                    center: point(px(self.right - r), px(self.bottom - r)),
                    radius: r,
                    start_angle: 0.0,
                },
            ),
            next(
                top_len,
                TrackComponentKind::Line {
                    from: point(px(self.right - r), px(self.bottom)),
                    to: point(px(self.left + r), px(self.bottom)),
                },
            ),
            next(
                arc_len,
                TrackComponentKind::Arc {
                    center: point(px(self.left + r), px(self.bottom - r)),
                    radius: r,
                    start_angle: std::f32::consts::FRAC_PI_2,
                },
            ),
            next(
                side_len,
                TrackComponentKind::Line {
                    from: point(px(self.left), px(self.bottom - r)),
                    to: point(px(self.left), px(self.top + r)),
                },
            ),
            next(
                arc_len,
                TrackComponentKind::Arc {
                    center: point(px(self.left + r), px(self.top + r)),
                    radius: r,
                    start_angle: std::f32::consts::PI,
                },
            ),
        ]
    }
}

#[derive(Clone, Copy, Debug)]
struct TrackComponent {
    offset: f32,
    length: f32,
    kind: TrackComponentKind,
}

impl TrackComponent {
    fn point_at(self, t: f32) -> gpui::Point<Pixels> {
        let t = t.clamp(0.0, 1.0);
        match self.kind {
            TrackComponentKind::Line { from, to } => point(
                px(f32::from(from.x) + (f32::from(to.x) - f32::from(from.x)) * t),
                px(f32::from(from.y) + (f32::from(to.y) - f32::from(from.y)) * t),
            ),
            TrackComponentKind::Arc {
                center,
                radius,
                start_angle,
            } => {
                let angle = start_angle + std::f32::consts::FRAC_PI_2 * t;
                point(
                    px(f32::from(center.x) + radius * angle.cos()),
                    px(f32::from(center.y) + radius * angle.sin()),
                )
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum TrackComponentKind {
    Line {
        from: gpui::Point<Pixels>,
        to: gpui::Point<Pixels>,
    },
    Arc {
        center: gpui::Point<Pixels>,
        radius: f32,
        start_angle: f32,
    },
}

#[cfg(test)]
#[path = "../../tests/unit/cards/snake_tests.rs"]
mod tests;
