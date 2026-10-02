use canvas::{layout::compute_project_region_layout, CanvasRect, Vec2};

/// Directory cards and camera anchors share this layout even before the first paint.
pub(super) fn workspace_layout(width: f32, height: f32, count: usize) -> Vec<CanvasRect> {
    let available = if width >= 940.0 {
        3
    } else if width >= 610.0 {
        2
    } else {
        1
    };
    let columns = available.min(count.max(1));
    let card_width =
        ((width - 112.0 - 16.0 * (available - 1) as f32) / available as f32).clamp(280.0, 460.0);
    let card_height = height * card_width / width.max(1.0);
    let grid_width = card_width * columns as f32 + 16.0 * (columns - 1) as f32;
    (0..count)
        .map(|index| {
            CanvasRect::new(
                (width - grid_width) * 0.5 + (index % columns) as f32 * (card_width + 16.0),
                118.0 + (index / columns) as f32 * (card_height + 16.0),
                card_width,
                card_height,
            )
        })
        .collect()
}

/// Stable catalog order: opening a workspace never changes its slot or prominence.
pub(super) fn home_layout(width: f32, height: f32, count: usize) -> Vec<CanvasRect> {
    let margin = if width >= 900.0 { 54.0 } else { 20.0 };
    let content = (width - margin * 2.0).clamp(1.0, 1344.0);
    let left = (width - content) * 0.5;
    let top = if width >= 640.0 { 202.0 } else { 246.0 };
    let wide = width >= 900.0;
    let hero = if wide {
        (height * 0.46).clamp(300.0, 440.0)
    } else {
        300.0
    };
    let gap = 12.0;
    (0..count)
        .map(|index| {
            if index == 0 {
                CanvasRect::new(
                    left,
                    top,
                    if wide {
                        (content - gap) * 0.62
                    } else {
                        content
                    },
                    hero,
                )
            } else if wide && index < 3 {
                let featured_width = (content - gap) * 0.62;
                CanvasRect::new(
                    left + featured_width + gap,
                    top + (index - 1) as f32 * (hero + gap) * 0.5,
                    content - featured_width - gap,
                    (hero - gap) * 0.5,
                )
            } else {
                let columns = if wide { 2 } else { 1 };
                let offset = if wide { index - 3 } else { index - 1 };
                let row_width = (content - gap * (columns - 1) as f32) / columns as f32;
                // The second section gets a label; narrow layouts keep the first three together.
                let section_gap = if wide || index >= 3 { 60.0 } else { gap };
                CanvasRect::new(
                    left + (offset % columns) as f32 * (row_width + gap),
                    top + hero + section_gap + (offset / columns) as f32 * 140.0,
                    row_width,
                    128.0,
                )
            }
        })
        .collect()
}

pub(super) fn home_preview_panel_rect(card: CanvasRect, featured: bool) -> CanvasRect {
    let padding = 12.0;
    let width = if featured {
        card.size.x - 2.0 * padding
    } else {
        (card.size.x * if card.size.y > 150.0 { 0.42 } else { 0.26 }).min(184.0)
    };
    let height = if featured {
        card.size.y - if card.size.x >= 600.0 { 74.0 } else { 88.0 }
    } else {
        card.size.y - 2.0 * padding
    };
    CanvasRect::new(
        card.origin.x + padding,
        card.origin.y + padding,
        width,
        height,
    )
}

/// Camera anchors are the actual uniformly scaled canvas, never the surrounding metadata card.
pub(super) fn home_preview_rect(
    card: CanvasRect,
    full: canvas::Vec2,
    featured: bool,
) -> CanvasRect {
    let panel = home_preview_panel_rect(card, featured);
    let zoom = (panel.size.x.max(1.0) / full.x).min(panel.size.y.max(1.0) / full.y);
    CanvasRect::new(
        panel.origin.x + (panel.size.x - full.x * zoom) * 0.5,
        panel.origin.y + (panel.size.y - full.y * zoom) * 0.5,
        full.x * zoom,
        full.y * zoom,
    )
}

// Use the settled window size for both the overview and its thumbnail; camera motion only scales it.
pub(super) fn project_area_layout(width: f32, height: f32, count: usize) -> Vec<CanvasRect> {
    let scene = super::desktop_scene_rect(width, height);
    let side = (width * 0.075).max(scene.origin.x);
    let top = (height * 0.06).max(scene.origin.y);
    let bottom = (height * 0.10).max(super::camera::DESKTOP_SCENE_BOTTOM);
    // At 1280×800: 96px side margins, 32px column gap and 64px row gap.
    // Only overview geometry changes; the expanded canvas keeps its original safe area.
    compute_project_region_layout(
        count,
        CanvasRect::new(
            side,
            top,
            (width - 2.0 * side).max(1.0),
            (height - top - bottom).max(1.0),
        ),
        Vec2::new(width * 0.025, height * 0.08),
    )
}

#[cfg(test)]
#[path = "../../../tests/unit/desktop_layout_tests.rs"]
mod tests;
