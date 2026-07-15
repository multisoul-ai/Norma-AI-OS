//! Shared card chrome primitives.

use canvas::{CanvasNode, CanvasRect, NodeType};
use gpui::{
    div, linear_color_stop, linear_gradient, point, prelude::*, px, rgba, AnyElement, Background,
    BoxShadow, Div, FontWeight,
};
use runtime::{NodeResize, Runtime};
use ui::Theme;

use super::{Card, CardBodyContext};
use crate::motion::breathing::{
    CornerGlow, CORNER_GLOW_BOTTOM_LEFT, CORNER_GLOW_BOTTOM_RIGHT, CORNER_GLOW_TOP_LEFT,
    CORNER_GLOW_TOP_RIGHT,
};
use crate::motion::{ResolvedNodeVisual, VisualPriority};

pub const TITLE_BAR_H: f32 = 44.0;

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

/// 四角光晕在屏幕上的最小可见尺寸（px）；卡片缩小到这个尺寸以下时跳过光晕绘制。
pub const CORNER_GLOW_MIN_SCREEN_PX: f32 = 32.0;

const CORNER_GLOW_OFFSET_PX: f32 = 6.0;
const CORNER_GLOW_SPREAD_PX: f32 = -2.0;

pub fn node_shadow(
    visual: ResolvedNodeVisual,
    zoom: f32,
    corner_glow_visible: bool,
) -> Vec<BoxShadow> {
    let mut shadows = Vec::with_capacity(5);
    shadows.push(BoxShadow {
        color: rgba(visual.shadow_color).into(),
        offset: point(px(0.0), px(visual.shadow_y)),
        blur_radius: px(visual.shadow_blur),
        spread_radius: px(visual.shadow_spread),
    });

    if visual.priority == VisualPriority::Focused {
        shadows.push(BoxShadow {
            color: rgba(with_alpha(visual.stroke_color, 0x38)).into(),
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(18.0),
            spread_radius: px(0.0),
        });
    }

    if corner_glow_visible {
        if let Some(glow) = visual.corner_glow {
            push_corner_glow(&mut shadows, glow, zoom);
        }
    }

    shadows
}

fn push_corner_glow(shadows: &mut Vec<BoxShadow>, glow: CornerGlow, zoom: f32) {
    let offset = CORNER_GLOW_OFFSET_PX * zoom;
    let blur = px(glow.blur * zoom);
    let spread = px(CORNER_GLOW_SPREAD_PX * zoom);
    let corners = [
        (CORNER_GLOW_TOP_LEFT, -offset, -offset),
        (CORNER_GLOW_TOP_RIGHT, offset, -offset),
        (CORNER_GLOW_BOTTOM_LEFT, -offset, offset),
        (CORNER_GLOW_BOTTOM_RIGHT, offset, offset),
    ];
    for (color, dx, dy) in corners {
        shadows.push(BoxShadow {
            color: rgba(with_glow_alpha(color, glow.alpha)).into(),
            offset: point(px(dx), px(dy)),
            blur_radius: blur,
            spread_radius: spread,
        });
    }
}

fn with_glow_alpha(color: u32, intensity: f32) -> u32 {
    let intensity = intensity.clamp(0.0, 1.0);
    let base_alpha = (color & 0xff) as f32;
    with_alpha(color, (base_alpha * intensity).round() as u8)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::motion::working::WorkingSnake;

    fn base_visual(corner_glow: Option<CornerGlow>) -> ResolvedNodeVisual {
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
            corner_glow,
            working_snake: None,
        }
    }

    /// CS-1: corner_glow=Some 且 corner_glow_visible=true 时，node_shadow 应在基础阴影之外追加 4 个角光晕。
    ///
    /// 数据构造（含关键数值推导）：
    ///   base_visual 的 priority=Idle（不触发 Focused 专属光晕），corner_glow.alpha=0.8、blur=10.0。
    ///   zoom=1.0，故角光晕的 offset/blur 不被缩放，便于直接断言数值。
    ///
    /// 执行过程：
    ///   1. 构造 corner_glow=Some(CornerGlow{blur:10.0, alpha:0.8})
    ///   2. 调用 node_shadow(visual, 1.0, true)
    ///
    /// 预期结果：
    ///   - 正断言：返回的 Vec 长度为 5（1 个基础阴影 + 4 个角光晕）
    ///   - 正断言：新增的 4 个 shadow 的 blur_radius 均为 10.0px
    ///   - 负断言：返回长度不应是 1（说明角光晕确实被追加）
    #[test]
    fn test_node_shadow_appends_four_corner_glows_when_visible() {
        let visual = base_visual(Some(CornerGlow {
            blur: 10.0,
            alpha: 0.8,
        }));

        let shadows = node_shadow(visual, 1.0, true);

        assert_eq!(
            shadows.len(),
            5,
            "Idle 优先级 + 可见的四角光晕应产出 5 个 shadow（1 基础 + 4 角），实际为 {}",
            shadows.len()
        );
        for glow_shadow in &shadows[1..] {
            assert_eq!(
                f32::from(glow_shadow.blur_radius),
                10.0,
                "四角光晕的 blur_radius 应等于 corner_glow.blur=10.0"
            );
        }
        assert_ne!(shadows.len(), 1, "追加四角光晕后不应仍只有 1 个基础阴影");
    }

    /// CS-2: corner_glow=None 或 corner_glow_visible=false 时，node_shadow 不应追加角光晕。
    ///
    /// 数据构造（含关键数值推导）：
    ///   场景 A：corner_glow=None（呼吸未激活）
    ///   场景 B：corner_glow=Some(...) 但 corner_glow_visible=false（屏幕尺寸低于裁剪阈值）
    ///
    /// 执行过程：
    ///   1. 场景 A：node_shadow(visual_without_glow, 1.0, true)
    ///   2. 场景 B：node_shadow(visual_with_glow, 1.0, false)
    ///
    /// 预期结果：
    ///   - 正断言：两个场景返回长度都为 1（只有基础阴影）
    ///   - 负断言：长度不应为 5（不应误触发角光晕）
    #[test]
    fn test_node_shadow_skips_corner_glow_when_absent_or_culled() {
        let without_glow = node_shadow(base_visual(None), 1.0, true);
        let culled = node_shadow(
            base_visual(Some(CornerGlow {
                blur: 10.0,
                alpha: 0.8,
            })),
            1.0,
            false,
        );

        assert_eq!(
            without_glow.len(),
            1,
            "corner_glow=None 时应只有基础阴影，实际为 {}",
            without_glow.len()
        );
        assert_eq!(
            culled.len(),
            1,
            "corner_glow_visible=false 时应只有基础阴影，实际为 {}",
            culled.len()
        );
        assert_ne!(without_glow.len(), 5, "corner_glow=None 时不应误产出角光晕");
        assert_ne!(culled.len(), 5, "裁剪命中时不应误产出角光晕");
    }

    /// CS-3: working_snake 只由 canvas overlay 绘制，不应追加卡片外发光 shadow。
    ///
    /// 数据构造（含关键数值推导）：
    ///   base_visual 的 priority=Idle（不触发 Focused 专属光晕），corner_glow=None。
    ///   working_snake.head_phase=0.25、tail_fraction=0.24 只用于表示 snake overlay 已存在。
    ///   基础阴影层数 = 1；若 working_snake 被错误用于卡片 chrome glow，则层数会变成 2。
    ///
    /// 执行过程：
    ///   1. 构造带 working_snake 的 Idle visual
    ///   2. 调用 node_shadow(visual, 1.0, true)
    ///
    /// 预期结果：
    ///   - 正断言：返回长度仍为 1，只保留基础阴影
    ///   - 正断言：基础阴影 blur_radius 仍为 20.0px
    ///   - 负断言：返回长度不应为 2，避免 snake 额外增加卡片外发光
    #[test]
    fn test_node_shadow_ignores_working_snake_overlay() {
        let mut visual = base_visual(None);
        visual.working_snake = Some(WorkingSnake {
            head_phase: 0.25,
            tail_fraction: 0.24,
            head_color: 0x27d7eae6,
            tail_color: 0x9b55ffe6,
            glow_color: 0x27d7eae6,
            inset_px: 3.0,
            stroke_px: 3.0,
        });

        let shadows = node_shadow(visual, 1.0, true);

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
