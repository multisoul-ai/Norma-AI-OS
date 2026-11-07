//! Display-independent island geometry, in AppKit's bottom-left coordinate space.
use super::IslandMode;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IslandFrame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IslandScreen {
    pub frame: IslandFrame,
    pub visible_top: f64,
    pub safe_top: f64,
    pub left_max_x: f64,
    pub right_min_x: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IslandLayout {
    pub frame: IslandFrame,
    pub notch_width: f64,
    pub header_height: f64,
    pub radius: f64,
}

impl IslandScreen {
    pub fn layout_for_content(self, mode: IslandMode, body_height: f64) -> IslandLayout {
        let mut layout = self.layout(mode);
        let height = if mode == IslandMode::Compact {
            layout.header_height
        } else {
            (layout.header_height + body_height.max(0.)).min(layout.frame.height)
        };
        layout.frame.y += layout.frame.height - height;
        layout.frame.height = height;
        layout.radius = layout.radius.min(height / 2.);
        layout
    }

    pub fn layout(self, mode: IslandMode) -> IslandLayout {
        let notch_width = if self.safe_top > 0. && self.right_min_x > self.left_max_x {
            self.right_min_x - self.left_max_x
        } else {
            0.
        };
        let notch_clearance = self.safe_top.max(34.);
        // The compact pill stays 34pt. Expanded keeps that clearance, then adds
        // 18pt so the title, workspace count and pin are not packed against the
        // tab bar.
        let header_height = if mode == IslandMode::Expanded {
            notch_clearance + 18.
        } else {
            notch_clearance
        };
        let (width, body, radius): (f64, f64, f64) = match mode {
            IslandMode::Compact => (330., 0., 999.),
            IslandMode::Activity => (460., 58., 24.),
            IslandMode::Alert => (540., 270., 32.),
            IslandMode::Expanded => (660., 510., 32.),
        };
        let top = if notch_width > 0. {
            self.frame.y + self.frame.height
        } else {
            // Without a notch the capsule floats near the very top edge, over the menu bar.
            self.frame.y + self.frame.height - 4.
        };
        let width = width.max(notch_width + 146.).min(self.frame.width);
        let height = (header_height + body).min((top - self.frame.y - 8.).max(header_height));
        let center = if notch_width > 0. {
            (self.left_max_x + self.right_min_x) / 2.
        } else {
            self.frame.x + self.frame.width / 2.
        };
        IslandLayout {
            frame: IslandFrame {
                x: (center - width / 2.)
                    .clamp(self.frame.x, self.frame.x + self.frame.width - width),
                y: top - height,
                width,
                height,
            },
            notch_width,
            header_height,
            radius: radius.min(height / 2.),
        }
    }
}

impl IslandLayout {
    /// Matches the rendered black surface, including square top corners at a notch.
    pub fn contains(self, x: f64, y: f64) -> bool {
        let x = x - self.frame.x;
        let y = y - self.frame.y;
        let (w, h, r) = (self.frame.width, self.frame.height, self.radius);
        if x < 0. || x > w || y < 0. || y > h {
            return false;
        }
        let corner_y = if y < r {
            r
        } else if self.notch_width == 0. && y > h - r {
            h - r
        } else {
            return true;
        };
        let corner_x = if x < r {
            r
        } else if x > w - r {
            w - r
        } else {
            return true;
        };
        (x - corner_x).powi(2) + (y - corner_y).powi(2) <= r * r
    }
}

#[cfg(test)]
#[path = "../../tests/unit/island_geometry.rs"]
mod tests;
