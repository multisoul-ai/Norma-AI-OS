//! Shared card chrome primitives.

use canvas::{CanvasNode, CanvasRect, NodeType};
use gpui::{div, point, prelude::*, px, rgba, AnyElement, BoxShadow, Div, FontWeight, IntoElement};
use runtime::{NodeResize, Runtime};
use ui::Theme;

use super::{Card, CardBodyContext};
use crate::motion::{ResolvedNodeVisual, VisualPriority};

pub const TITLE_BAR_H: f32 = 26.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardChrome {
    pub title: String,
    pub refreshable: bool,
    pub task_badge: Option<CardChromeTaskBadge>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardChromeTaskTone {
    Muted,
    Active,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardChromeTaskBadge {
    pub label: String,
    pub tone: CardChromeTaskTone,
}

pub fn render_task_badge(badge: CardChromeTaskBadge, zoom: f32, theme: Theme) -> Div {
    let (fg, bg) = match badge.tone {
        CardChromeTaskTone::Muted => (theme.text_secondary, with_alpha(theme.panel_high, 0x68)),
        CardChromeTaskTone::Active => (theme.accent_cyan, with_alpha(theme.accent_cyan, 0x18)),
        CardChromeTaskTone::Warning => (theme.accent_warm, with_alpha(theme.accent_warm, 0x1f)),
        CardChromeTaskTone::Error => (theme.destructive, with_alpha(theme.destructive, 0x1f)),
    };
    div()
        .px(px(6.0 * zoom))
        .h(px((16.0 * zoom).clamp(14.0, 18.0)))
        .flex()
        .items_center()
        .rounded(px((8.0 * zoom).clamp(7.0, 9.0)))
        .bg(rgba(bg))
        .text_color(rgba(fg))
        .text_size(px((9.0 * zoom).clamp(8.0, 10.0)))
        .font_weight(FontWeight::MEDIUM)
        .child(badge.label)
}

pub fn node_shadow(visual: ResolvedNodeVisual) -> Vec<BoxShadow> {
    let mut shadows = vec![BoxShadow {
        color: rgba(visual.shadow_color).into(),
        offset: point(px(0.0), px(visual.shadow_y)),
        blur_radius: px(visual.shadow_blur),
        spread_radius: px(visual.shadow_spread),
    }];

    if visual.priority == VisualPriority::Focused {
        shadows.push(BoxShadow {
            color: rgba(with_alpha(visual.stroke_color, 0x38)).into(),
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(18.0),
            spread_radius: px(0.0),
        });
    }

    shadows
}

pub struct EmptyCard;

pub static EMPTY_CARD: EmptyCard = EmptyCard;

impl Card for EmptyCard {
    fn ty(&self) -> NodeType {
        NodeType::Rect
    }

    fn chrome(&self, node: &CanvasNode, _runtime: &Runtime) -> CardChrome {
        CardChrome {
            title: node.kind.display_title(),
            refreshable: false,
            task_badge: None,
        }
    }

    fn body(&self, _node: &CanvasNode, _context: CardBodyContext<'_>) -> AnyElement {
        div().into_any_element()
    }

    fn resize(&self, _bounds: CanvasRect, _zoom: f32) -> Option<NodeResize> {
        None
    }
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
