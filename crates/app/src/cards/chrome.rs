//! Shared card chrome primitives.

use canvas::{CanvasNode, CanvasRect, NodeId, NodeType};
use gpui::{
    div, linear_color_stop, linear_gradient, point, prelude::*, px, rgba, AnyElement, Background,
    BoxShadow, Div, FontWeight,
};
use runtime::{NodeResize, Runtime};
use ui::Theme;

use super::{Card, CardBodyContext};
use crate::motion::{ResolvedNodeVisual, VisualPriority};

pub const TITLE_BAR_H: f32 = 32.0;

/// A translucent cool-white gradient layered over the card shell. GPUI 0.2.2
/// has no per-element backdrop blur, so the low-alpha shell, chromatic gradient,
/// keyline and wallpaper beneath it cooperate to produce the frosted material.
pub fn card_title_background(title_color: u32, theme: Theme) -> Background {
    linear_gradient(
        165.0,
        linear_color_stop(rgba(title_color), 0.0),
        linear_color_stop(rgba(with_alpha(theme.canvas_to, 0x38)), 1.0),
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardChrome {
    pub title: String,
    pub title_detail: Option<String>,
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

pub fn render_card_title(
    node_id: NodeId,
    title: String,
    title_detail: Option<String>,
    zoom: f32,
    theme: Theme,
) -> AnyElement {
    let title = div()
        .debug_selector(|| "card-title".to_string())
        .overflow_hidden()
        .truncate()
        .font_weight(FontWeight::MEDIUM)
        .child(title);
    let content = div()
        .id(("node-title", node_id))
        .flex()
        .flex_row()
        .items_center()
        .flex_1()
        .min_w(px(0.0))
        .overflow_hidden()
        .gap(px(5.0 * zoom));

    match title_detail {
        Some(detail) => content
            .child(title.flex_none())
            .child(
                div()
                    .flex_none()
                    .text_color(rgba(theme.text_tertiary))
                    .child("—"),
            )
            .child(
                div()
                    .debug_selector(|| "card-title-detail".to_string())
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .truncate()
                    .text_color(rgba(theme.text_secondary))
                    .child(detail),
            ),
        None => content.child(title.flex_1().min_w(px(0.0))),
    }
    .into_any_element()
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
    let mut shadows = Vec::with_capacity(2);
    shadows.push(BoxShadow {
        color: rgba(visual.shadow_color).into(),
        offset: point(px(0.0), px(visual.shadow_y)),
        blur_radius: px(visual.shadow_blur),
        spread_radius: px(visual.shadow_spread),
    });

    if visual.priority == VisualPriority::Focused && !visual.breathing_border_active {
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
            title_detail: None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::motion::working::WorkingSnake;

    fn base_visual(breathing_border_active: bool) -> ResolvedNodeVisual {
        ResolvedNodeVisual {
            priority: VisualPriority::Idle,
            rect: CanvasRect::new(0.0, 0.0, 100.0, 80.0),
            fill_color: 0x000000ff,
            stroke_color: 0xffffffff,
            status_color: 0xffffffff,
            title_bar_color: 0x000000ff,
            edge_highlight_color: 0xffffffff,
            shadow_color: 0x000000aa,
            shadow_y: 4.0,
            shadow_blur: 20.0,
            shadow_spread: -10.0,
            border_is_emphasized: false,
            opacity: 1.0,
            breathing_border_active,
            border_glow: None,
            working_snake: None,
        }
    }

    /// CS-1: 完成提示的呼吸色仅画在描边，不能用整卡阴影染亮卡片内容。
    ///
    /// 数据构造（含关键数值推导）：
    ///   base_visual 的 priority=Idle，breathing_border_active=true，基础阴影 blur=20px。
    ///
    /// 执行过程：
    ///   1. 构造正在呼吸的卡片视觉。
    ///   2. 调用 node_shadow(visual)。
    ///
    /// 预期结果：
    ///   - 正断言：仅返回 1 个基础阴影，模糊半径保持 20px
    ///   - 负断言：不返回原先的 5 层阴影，避免霓虹色覆盖整卡
    #[test]
    fn test_node_shadow_keeps_breathing_effect_on_border_only() {
        let visual = base_visual(true);
        let shadows = node_shadow(visual);

        assert_eq!(
            shadows.len(),
            1,
            "完成态呼吸只应影响描边，阴影应只有基础层，实际为 {}",
            shadows.len()
        );
        assert_eq!(
            f32::from(shadows[0].blur_radius),
            20.0,
            "基础阴影应保持 20px"
        );
        assert_ne!(shadows.len(), 5, "完成态不得叠加四层整卡彩色光晕");
    }

    /// CS-2: 聚焦且正在呼吸的 Agent 不能产生随描边变色的整卡聚焦光晕。
    ///
    /// 数据构造（含关键数值推导）：
    ///   场景 A：Focused + breathing_border_active=true，基础阴影 1 层。
    ///   场景 B：Focused + breathing_border_active=false，基础阴影 + 聚焦阴影 2 层。
    ///
    /// 执行过程：
    ///   1. 分别解析聚焦的呼吸卡片和普通卡片阴影。
    ///
    /// 预期结果：
    ///   - 正断言：呼吸卡片 1 层、普通聚焦卡片 2 层。
    ///   - 负断言：呼吸卡片不能带第二层彩色聚焦阴影。
    #[test]
    fn test_node_shadow_skips_focused_glow_while_border_breathes() {
        let mut breathing = base_visual(true);
        breathing.priority = VisualPriority::Focused;
        let mut focused = base_visual(false);
        focused.priority = VisualPriority::Focused;
        let breathing_shadows = node_shadow(breathing);
        let focused_shadows = node_shadow(focused);

        assert_eq!(
            breathing_shadows.len(),
            1,
            "聚焦的呼吸卡片应只有基础阴影，实际为 {}",
            breathing_shadows.len()
        );
        assert_eq!(
            focused_shadows.len(),
            2,
            "普通聚焦卡片应保留原有两层阴影，实际为 {}",
            focused_shadows.len()
        );
        assert_ne!(breathing_shadows.len(), 2, "呼吸卡片不得带整卡彩色聚焦光晕");
    }

    /// CS-3: working_snake 只由 canvas overlay 绘制，不应追加卡片外发光 shadow。
    ///
    /// 数据构造（含关键数值推导）：
    ///   base_visual 的 priority=Idle（不触发 Focused 专属光晕），呼吸关闭。
    ///   working_snake.head_phase=0.25、tail_fraction=0.24 只用于表示 snake overlay 已存在。
    ///   基础阴影层数 = 1；若 working_snake 被错误用于卡片 chrome glow，则层数会变成 2。
    ///
    /// 执行过程：
    ///   1. 构造带 working_snake 的 Idle visual
    ///   2. 调用 node_shadow(visual)
    ///
    /// 预期结果：
    ///   - 正断言：返回长度仍为 1，只保留基础阴影
    ///   - 正断言：基础阴影 blur_radius 仍为 20.0px
    ///   - 负断言：返回长度不应为 2，避免 snake 额外增加卡片外发光
    #[test]
    fn test_node_shadow_ignores_working_snake_overlay() {
        let mut visual = base_visual(false);
        visual.working_snake = Some(WorkingSnake {
            head_phase: 0.25,
            tail_fraction: 0.24,
            head_color: 0x27d7eae6,
            tail_color: 0x9b55ffe6,
            glow_color: 0x27d7eae6,
            inset_px: 3.0,
            stroke_px: 3.0,
        });

        let shadows = node_shadow(visual);

        assert_eq!(
            shadows.len(),
            1,
            "working_snake 应只通过 overlay 绘制，不应让 node_shadow 追加外发光"
        );
        assert_eq!(
            f32::from(shadows[0].blur_radius),
            20.0,
            "基础阴影 blur_radius 应保持 base_visual 的 20.0px"
        );
        assert_ne!(
            shadows.len(),
            2,
            "working_snake 不应额外增加第二层 shadow，否则卡片本体仍在动画变色/发光"
        );
    }
}
