//! Shared card chrome primitives.

use crate::motion::{ResolvedNodeVisual, VisualPriority};
use canvas::{CanvasNode, CanvasRect, NodeType};
use gpui::{div, point, px, rgba, AnyElement, BoxShadow, Context, IntoElement, Window};
use runtime::{NodeResize, Runtime};
use ui::Theme;

use super::Card;
use crate::app::MultisoulApp;

pub const TITLE_BAR_H: f32 = 26.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CardChrome {
    pub title: &'static str,
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
        }
    }

    fn body(
        &self,
        _node: &CanvasNode,
        _runtime: &Runtime,
        _theme: Theme,
        _zoom: f32,
        _bottom_radius_px: f32,
        _window: &mut Window,
        _cx: &mut Context<MultisoulApp>,
    ) -> AnyElement {
        div().into_any_element()
    }

    fn resize(&self, _bounds: CanvasRect, _zoom: f32) -> Option<NodeResize> {
        None
    }
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
