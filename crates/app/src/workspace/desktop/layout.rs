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

/// Combo's central stack is vertically balanced, with room for window controls.
pub(super) fn home_brand_top(height: f32) -> f32 {
    ((height - 508.0) * 0.5).max(84.0)
}

/// Stable equal-size recent cards. Small catalogs reserve a final creation slot.
pub(super) fn home_layout(width: f32, height: f32, count: usize) -> Vec<CanvasRect> {
    home_slots(width, height, count)
        .into_iter()
        .take(count.min(4))
        .collect()
}

pub(super) fn home_slots(width: f32, height: f32, count: usize) -> Vec<CanvasRect> {
    let slots = count.min(4) + usize::from((1..=2).contains(&count));
    if slots == 0 {
        return Vec::new();
    }
    let card_width = 252.0_f32.min((width - 40.0).max(1.0));
    let columns = (((width - 24.0) / (card_width + 16.0)).floor() as usize).clamp(1, slots);
    let grid_width = columns as f32 * card_width + (columns - 1) as f32 * 16.0;
    (0..slots)
        .map(|index| {
            CanvasRect::new(
                (width - grid_width) * 0.5 + (index % columns) as f32 * (card_width + 16.0),
                home_brand_top(height) + 270.0 + (index / columns) as f32 * 220.0,
                card_width,
                204.0,
            )
        })
        .collect()
}

pub(super) fn home_preview_panel_rect(card: CanvasRect, _featured: bool) -> CanvasRect {
    // 1px outer border plus 6px inset. Opaque preview owns its own 16px radii.
    CanvasRect::new(
        card.origin.x + 6.0,
        card.origin.y + 6.0,
        (card.size.x - 14.0).max(1.0),
        128.0,
    )
}

/// Fit the project's real arrangement to the whole home preview panel.
pub(super) fn home_project_preview_layout(size: Vec2, areas: &[CanvasRect]) -> Vec<CanvasRect> {
    let Some(first) = areas.first() else {
        return Vec::new();
    };
    let (min_x, min_y, max_x, max_y) = areas.iter().fold(
        (first.origin.x, first.origin.y, first.max().x, first.max().y),
        |(min_x, min_y, max_x, max_y), rect| {
            (
                min_x.min(rect.origin.x),
                min_y.min(rect.origin.y),
                max_x.max(rect.max().x),
                max_y.max(rect.max().y),
            )
        },
    );
    let panel = CanvasRect::new(0.0, 0.0, size.x, size.y).inset(8.0);
    let zoom =
        (panel.size.x / (max_x - min_x).max(1.0)).min(panel.size.y / (max_y - min_y).max(1.0));
    let left = panel.origin.x + (panel.size.x - (max_x - min_x) * zoom) * 0.5;
    let top = panel.origin.y + (panel.size.y - (max_y - min_y) * zoom) * 0.5;
    areas
        .iter()
        .map(|rect| {
            CanvasRect::new(
                left + (rect.origin.x - min_x) * zoom,
                top + (rect.origin.y - min_y) * zoom,
                rect.size.x * zoom,
                rect.size.y * zoom,
            )
        })
        .collect()
}

// Use the settled window size for both the overview and its thumbnail; camera motion only scales it.
pub(super) fn project_area_layout(width: f32, height: f32, count: usize) -> Vec<CanvasRect> {
    let scene = super::desktop_scene_rect(width, height);
    let side = 24.0_f32.max(scene.origin.x);
    // Overview and expanded projects share the title bar's safe boundary.
    let top = (height * 0.06).max(scene.origin.y);
    let bottom = (height * 0.10).max(super::camera::DESKTOP_SCENE_BOTTOM);
    // Compact spacing leaves more room for the live previews: 24px sides,
    // 16px between columns and 24px between rows, with canvas proportions intact.
    // Only overview geometry changes; the expanded canvas keeps its original safe area.
    let area = CanvasRect::new(
        side,
        top,
        (width - 2.0 * side).max(1.0),
        (height - top - bottom).max(1.0),
    );
    if (1..=2).contains(&count) {
        let gap = if count == 2 {
            16.0_f32.min(area.size.x / 2.0)
        } else {
            0.0
        };
        let card_width = ((area.size.x - gap) / count as f32).min(area.size.y * 16.0 / 9.0);
        let card_height = card_width * 9.0 / 16.0;
        let left = area.origin.x + (area.size.x - count as f32 * card_width - gap) * 0.5;
        let y = area.origin.y + (area.size.y - card_height) * 0.5;
        return (0..count)
            .map(|index| {
                CanvasRect::new(
                    left + index as f32 * (card_width + gap),
                    y,
                    card_width,
                    card_height,
                )
            })
            .collect();
    }
    compute_project_region_layout(count, area, Vec2::new(16.0, 24.0))
}

#[cfg(test)]
#[path = "../../../tests/unit/desktop_layout_tests.rs"]
mod tests;
