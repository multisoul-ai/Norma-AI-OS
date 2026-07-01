//! Shared card chrome primitives.

use canvas::{CanvasNode, CanvasRect, NodeType, Vec2};
use gpui::{
    div, linear_color_stop, linear_gradient, point, prelude::*, px, rgba, AnyElement, BoxShadow,
    Div, FontWeight,
};
use runtime::{NodeResize, Runtime};
use ui::Theme;

use super::{Card, CardBodyContext};
use crate::motion::breathing::{
    chase_edge_segment, ChaseGlow, CornerGlow, CHASE_TAIL_LENGTH, CORNER_GLOW_BOTTOM_LEFT,
    CORNER_GLOW_BOTTOM_RIGHT, CORNER_GLOW_TOP_LEFT, CORNER_GLOW_TOP_RIGHT,
};
use crate::motion::{ResolvedNodeVisual, VisualPriority};

pub const TITLE_BAR_H: f32 = 26.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardChrome {
    pub title: String,
    pub refreshable: bool,
    pub task_badge: Option<CardChromeTaskBadge>,
    pub status_line: Option<String>,
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
    let mut shadows = Vec::with_capacity(6);
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

/// 返回蛇形覆盖层容器，与卡片同位置但无 overflow_hidden，使圆角弧上的小圆不被剪裁。
pub fn chase_overlay_container(
    pos: Vec2,
    w: f32,
    h: f32,
    chase: ChaseGlow,
    zoom: f32,
) -> AnyElement {
    div()
        .absolute()
        .left(px(pos.x))
        .top(px(pos.y))
        .w(px(w))
        .h(px(h))
        .children(chase_snake_overlays(chase, w, h, zoom))
        .into_any_element()
}

/// 生成 snake 追尾光 overlay：4 条边（各收缩圆角距离）+ 最多 4 个角点圆点。
fn chase_snake_overlays(chase: ChaseGlow, w: f32, h: f32, zoom: f32) -> Vec<Div> {
    let t = 3.5 * zoom;
    let r = (16.0 * zoom).clamp(8.0, 22.0);
    let alpha = (0xCC as f32 * chase.fade).round() as u8;
    let bright = rgba(0x27d7ea00_u32 | u32::from(alpha));
    let dim = rgba(0x27d7ea00_u32);
    let iw = (w - 2.0 * r).max(0.0);
    let ih = (h - 2.0 * r).max(0.0);
    let mut out: Vec<Div> = Vec::new();

    // top: TL→TR (90°)，strip 从 x=r 到 x=w-r
    if let Some((lt, lh)) = chase_edge_segment(chase.phase, CHASE_TAIL_LENGTH, 0.0, 0.25) {
        out.push(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(r + lt * iw))
                .w(px((lh - lt) * iw))
                .h(px(t))
                .rounded(px(t / 2.0))
                .bg(linear_gradient(
                    90.0,
                    linear_color_stop(dim, 0.0),
                    linear_color_stop(bright, 1.0),
                )),
        );
    }
    // right: TR→BR (180°)，strip 从 y=r 到 y=h-r
    if let Some((lt, lh)) = chase_edge_segment(chase.phase, CHASE_TAIL_LENGTH, 0.25, 0.5) {
        out.push(
            div()
                .absolute()
                .right(px(0.0))
                .top(px(r + lt * ih))
                .w(px(t))
                .h(px((lh - lt) * ih))
                .rounded(px(t / 2.0))
                .bg(linear_gradient(
                    180.0,
                    linear_color_stop(dim, 0.0),
                    linear_color_stop(bright, 1.0),
                )),
        );
    }
    // bottom: BR→BL (270°)，strip 从 x=w-r 到 x=r
    if let Some((lt, lh)) = chase_edge_segment(chase.phase, CHASE_TAIL_LENGTH, 0.5, 0.75) {
        out.push(
            div()
                .absolute()
                .bottom(px(0.0))
                .left(px(r + (1.0 - lh) * iw))
                .w(px((lh - lt) * iw))
                .h(px(t))
                .rounded(px(t / 2.0))
                .bg(linear_gradient(
                    270.0,
                    linear_color_stop(dim, 0.0),
                    linear_color_stop(bright, 1.0),
                )),
        );
    }
    // left: BL→TL (0°)，strip 从 y=h-r 到 y=r
    if let Some((lt, lh)) = chase_edge_segment(chase.phase, CHASE_TAIL_LENGTH, 0.75, 1.0) {
        out.push(
            div()
                .absolute()
                .left(px(0.0))
                .top(px(r + (1.0 - lh) * ih))
                .w(px(t))
                .h(px((lh - lt) * ih))
                .rounded(px(t / 2.0))
                .bg(linear_gradient(
                    0.0,
                    linear_color_stop(dim, 0.0),
                    linear_color_stop(bright, 1.0),
                )),
        );
    }

    // 角部圆弧：K 个小圆沿卡片圆角弧线排列，snake 经过时显示。
    // 每个小圆圆心用 sin/cos 精确落在卡片圆弧上，整体看起来是 snake 绕圆角行进。
    // 渐变方向：arc_t=1（出边）最亮 → arc_t=0（入边）稍暗（对应 head→tail 方向）。
    const K: usize = 8;
    const CORNER_W: f32 = 0.04; // 角部虚拟宽度（perimeter 坐标），用于产生 head→tail 渐变
    let half_pi: f32 = std::f32::consts::FRAC_PI_2;
    for (cp, is_right, is_bottom) in [
        (0.0_f32, false, false), // TL
        (0.25, true, false),     // TR
        (0.5, true, true),       // BR
        (0.75, false, true),     // BL
    ] {
        let tail = chase.phase - CHASE_TAIL_LENGTH;
        let hit = if tail >= 0.0 {
            tail <= cp && cp <= chase.phase
        } else {
            cp >= tail + 1.0 || cp <= chase.phase
        };
        if !hit {
            continue;
        }
        let dist = (chase.phase - cp).rem_euclid(1.0);
        for i in 0..K {
            let frac = (i as f32 + 0.5) / K as f32; // arc_t: 0=入边, 1=出边
            let seg_dist = dist + (1.0 - frac) * CORNER_W;
            let seg_intens = (1.0 - seg_dist / CHASE_TAIL_LENGTH).clamp(0.0, 1.0);
            let seg_a = (0xCC as f32 * chase.fade * seg_intens).round() as u8;
            if seg_a == 0 {
                continue;
            }
            let seg_col = rgba(0x27d7ea00_u32 | u32::from(seg_a));
            let alpha = frac * half_pi;
            // 各角圆弧公式（arc center: TL=(r,r), TR=(w-r,r), BR=(w-r,h-r), BL=(r,h-r)）
            let (sx, sy) = match (is_right, is_bottom) {
                (false, false) => (r * (1.0 - alpha.cos()), r * (1.0 - alpha.sin())),
                (true, false) => (w - r + r * alpha.sin(), r * (1.0 - alpha.cos())),
                (true, true) => (w - r + r * alpha.cos(), h - r + r * alpha.sin()),
                _ => (r * (1.0 - alpha.sin()), h - r + r * alpha.cos()),
            };
            out.push(
                div()
                    .absolute()
                    .left(px(sx - t / 2.0))
                    .top(px(sy - t / 2.0))
                    .w(px(t))
                    .h(px(t))
                    .rounded(px(t / 2.0))
                    .bg(seg_col),
            );
        }
    }

    out
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
            status_line: None,
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
            chase_glow: None,
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
}
