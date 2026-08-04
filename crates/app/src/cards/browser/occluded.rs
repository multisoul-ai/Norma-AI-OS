//! 被浮层（布局菜单）遮挡的浏览器卡片中性背景。

use canvas::NodeId;
use gpui::{div, prelude::*, px, rgba, AnyElement};
use ui::icons::with_alpha;
use ui::Theme;

/// 被布局菜单遮挡时渲染的中性背景。原生子视图已被隐藏，这里用与卡片一致的
/// 面板色填充，避免显示 "Loading browser..." 占位文字误导用户。
pub(crate) fn occluded_browser_body(
    node_id: NodeId,
    theme: Theme,
    bottom_radius_px: f32,
) -> AnyElement {
    div()
        .id(("browser-card-occluded", node_id))
        .debug_selector(|| "browser-card-occluded".to_string())
        .size_full()
        .bg(rgba(with_alpha(theme.panel_high, 0x72)))
        .rounded_b(px(bottom_radius_px))
        .into_any_element()
}
