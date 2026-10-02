//! Project previews present the original live canvas at a uniform scale.
use super::{GLASS_TEXT, GLASS_TEXT_MUTED};
use crate::app::MultisoulApp;
use canvas::{CanvasRect, Vec2};
use gpui::{div, prelude::*, px, rgba, Context, Div};
use storage::DesktopAreaPreview;
use ui::component::StyledExt;
use ui::icons::with_alpha;

/// Keep chrome inside tiny thumbnails as well as full-size project regions.
pub(super) fn project_chrome_scale(size: Vec2, scale: f32) -> f32 {
    scale
        .max(0.0)
        .min(size.x.max(0.0) / 56.0)
        .min(size.y.max(0.0) / 56.0)
}

pub(super) fn project_stage_rect(size: Vec2, scale: f32) -> CanvasRect {
    let chrome = project_chrome_scale(size, scale);
    CanvasRect::new(
        8.0 * chrome,
        28.0 * chrome,
        (size.x - 16.0 * chrome).max(0.0),
        (size.y - 36.0 * chrome).max(0.0),
    )
}

pub(super) fn project_tone(label: &str, theme: ui::Theme) -> u32 {
    let project = label.split(" / ").next().unwrap_or(label);
    let hash = project.bytes().fold(0u32, |value, byte| {
        value.wrapping_mul(31).wrapping_add(u32::from(byte))
    });
    [
        theme.accent_violet,
        theme.accent_green,
        theme.accent_blue,
        theme.accent_cyan,
    ][hash as usize % 4]
}

/// Fit the entire logical canvas, including its empty space, below the project title.
pub(super) fn project_canvas_rect(card: CanvasRect, logical: Vec2, scale: f32) -> CanvasRect {
    let stage = project_stage_rect(card.size, scale);
    let zoom = (stage.size.x / logical.x.max(1.0)).min(stage.size.y / logical.y.max(1.0));
    let size = Vec2::new(logical.x * zoom, logical.y * zoom);
    CanvasRect {
        origin: card.origin
            + stage.origin
            + Vec2::new((stage.size.x - size.x) * 0.5, (stage.size.y - size.y) * 0.5),
        size,
    }
}

impl MultisoulApp {
    pub(super) fn render_desktop_project_preview(
        &self,
        workspace_id: &str,
        area: &DesktopAreaPreview,
        width: f32,
        height: f32,
        scale: f32,
        logical_size: Vec2,
        selector: String,
        cx: &mut Context<Self>,
    ) -> Div {
        let stage = project_stage_rect(Vec2::new(width, height), scale);
        let chrome = project_chrome_scale(Vec2::new(width, height), scale);
        let theme = self.desktop_theme();
        div()
            .relative()
            .w(px(width))
            .h(px(height))
            .rounded(px(14.0 * chrome))
            .border_1()
            .border_color(rgba(theme.dock_line))
            .bg(rgba(with_alpha(theme.dock, 0x8c)))
            .child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .right(px(0.0))
                    .h(px(stage.origin.y))
                    .px(px(8.0 * chrome))
                    .flex()
                    .items_center()
                    .font_semibold()
                    .text_size(px(13.0 * chrome))
                    .text_color(rgba(GLASS_TEXT))
                    .overflow_hidden()
                    .child(div().min_w(px(0.0)).truncate().child(area.name.clone())),
            )
            .child(self.desktop_project_canvas_preview(
                workspace_id,
                &area.id,
                Vec2::new(width, height),
                logical_size,
                scale,
                cx,
            ))
            // The directory is navigation only: a tiny close button must not close a live node.
            .child(div().absolute().inset_0().occlude())
            .debug_selector(move || selector.clone())
    }

    pub(super) fn desktop_project_canvas_preview(
        &self,
        workspace_id: &str,
        area_id: &str,
        card_size: Vec2,
        logical_size: Vec2,
        scale: f32,
        _cx: &mut Context<Self>,
    ) -> Div {
        let frame = project_canvas_rect(
            CanvasRect {
                origin: Vec2::ZERO,
                size: card_size,
            },
            logical_size,
            scale,
        );
        let mut content = div()
            .absolute()
            // The shell's 1px border insets its positioned children; frame is in outer coordinates.
            .left(px(frame.origin.x - 1.0))
            .top(px(frame.origin.y - 1.0))
            .w(px(frame.size.x))
            .h(px(frame.size.y))
            .overflow_hidden();
        if let Some(session) = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.to_owned(), area_id.to_owned()))
        {
            content = content.child(super::project_world::project_canvas_scene(
                session.clone(),
                logical_size,
                frame.size.x / logical_size.x,
            ));
        } else {
            let loading = self
                .desktop_workspace
                .loading_area_ids
                .contains(&(workspace_id.to_owned(), area_id.to_owned()));
            content = content
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.0 * scale))
                .text_color(rgba(GLASS_TEXT_MUTED))
                .child(if loading {
                    "Loading canvas…"
                } else {
                    "Open workspace to load canvas"
                });
        }
        content
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/desktop_live_preview_tests.rs"]
mod live_preview_tests;

#[cfg(test)]
mod tests {
    use super::*;

    /// 项目身份色只取项目名：同一项目的不同 worktree 使用同一种颜色。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   alpha 的两个标签共享项目名前缀；beta 是另一项目；调色板含 4 种颜色。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 分别计算 alpha / main、alpha / feature 与 beta / main 的颜色。
    ///   2. 比较同项目和不同项目的结果。
    ///
    /// 预期结果：
    ///   - 正断言：alpha 的两张卡片颜色相同。
    ///   - 负断言：beta 不使用 alpha 的颜色，避免相邻项目难以区分。
    #[test]
    fn project_tone_uses_project_identity_across_worktrees() {
        let theme = ui::Theme::sapphire_night();
        let alpha = project_tone("alpha / main", theme);
        assert_eq!(
            alpha,
            project_tone("alpha / feature", theme),
            "同一项目的不同 worktree 必须保留一致的身份色"
        );
        assert_ne!(
            alpha,
            project_tone("beta / main", theme),
            "不同项目应使用不同身份色，以便跨项目总览辨认"
        );
    }
}
