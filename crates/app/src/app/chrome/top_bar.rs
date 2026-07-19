//! Full-width workspace title bar aligned with the reviewed Product UI canvas chrome.

use crate::app::MultisoulApp;
use crate::workspace::persistence::PersistenceSaveState;
use gpui::{
    div, point, prelude::*, px, rgba, BoxShadow, Context, Div, Stateful, WindowControlArea,
};
use std::path::Path;
use ui::component::StyledExt;
use workspace::display_name_for_path;

pub(super) const TOP_BAR_HEIGHT: f32 = 54.0;
pub(super) const TOP_BAR_FILL: u32 = 0x0d131f4d;
pub(super) const TOP_BAR_BORDER: u32 = 0xffffff29;
const TOP_BAR_FOREGROUND: u32 = 0xffffffe6;
const TOP_BAR_MUTED: u32 = 0xffffff7a;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CanvasSaveVisual {
    pub(super) label: &'static str,
    pub(super) dot: u32,
}

pub(super) fn canvas_save_visual(pending: bool, failed: bool) -> CanvasSaveVisual {
    if failed {
        CanvasSaveVisual {
            label: "Save issue",
            dot: 0xff6b7aff,
        }
    } else if pending {
        CanvasSaveVisual {
            label: "Saving…",
            dot: 0xd6ff3bff,
        }
    } else {
        CanvasSaveVisual {
            label: "Canvas saved",
            dot: 0x64e0b5ff,
        }
    }
}

impl MultisoulApp {
    pub(super) fn render_workspace_top_bar(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let project_name = workspace_top_bar_title(self.workspace_state.current_root.as_deref());
        let save = match self.persistence.save_state() {
            PersistenceSaveState::Clean => canvas_save_visual(false, false),
            PersistenceSaveState::Pending => canvas_save_visual(true, false),
            PersistenceSaveState::Failed => canvas_save_visual(false, true),
        };

        div()
            .id("workspace-top-bar")
            .debug_selector(|| "workspace-top-bar".to_string())
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .h(px(TOP_BAR_HEIGHT))
            .flex()
            .items_center()
            .justify_center()
            .border_b_1()
            .border_color(rgba(TOP_BAR_BORDER))
            .bg(rgba(TOP_BAR_FILL))
            .shadow(top_bar_shadow())
            .window_control_area(WindowControlArea::Drag)
            .child(workspace_title(project_name))
            .child(self.workspace_top_bar_actions(save, cx))
    }

    fn workspace_top_bar_actions(&mut self, save: CanvasSaveVisual, cx: &mut Context<Self>) -> Div {
        div()
            .absolute()
            .top(px(0.0))
            .right(px(18.0))
            .h(px(TOP_BAR_HEIGHT))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                launchpad_button().on_click(cx.listener(|_app, _, _window, cx| {
                    MultisoulApp::open_new_launchpad_window(cx);
                })),
            )
            .child(save_status(save))
    }
}

fn workspace_top_bar_title(path: Option<&Path>) -> String {
    path.map(display_name_for_path).unwrap_or_default()
}

fn workspace_title(path: String) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_size(px(12.0))
        .child(
            div()
                .font_semibold()
                .text_color(rgba(TOP_BAR_FOREGROUND))
                .child("Norma OS"),
        )
        .child(
            div()
                .max_w(px(360.0))
                .overflow_hidden()
                .truncate()
                .font_family("Menlo")
                .text_size(px(9.0))
                .text_color(rgba(TOP_BAR_MUTED))
                .child(path),
        )
}

fn launchpad_button() -> Stateful<Div> {
    div()
        .id("workspace-launchpad-button")
        .debug_selector(|| "workspace-launchpad-button".to_string())
        .w(px(38.0))
        .h(px(38.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(12.0))
        .border_1()
        .border_color(rgba(0xffffff2e))
        .bg(rgba(0xffffff1a))
        .cursor_pointer()
        .hover(|style| style.bg(rgba(0xffffff29)))
        .active(|style| style.bg(rgba(0xffffff38)))
        .child(launchpad_grid_glyph())
}

fn launchpad_grid_glyph() -> Div {
    div()
        .grid()
        .grid_cols(3)
        .grid_rows(3)
        .gap(px(2.0))
        .children((0..9).map(|_| {
            div()
                .w(px(2.0))
                .h(px(2.0))
                .rounded(px(1.0))
                .bg(rgba(0xffffffd6))
        }))
}

fn save_status(save: CanvasSaveVisual) -> Stateful<Div> {
    div()
        .id("workspace-save-status")
        .debug_selector(|| "workspace-save-status".to_string())
        .h(px(28.0))
        .flex()
        .items_center()
        .gap(px(7.0))
        .px(px(10.0))
        .rounded(px(14.0))
        .bg(rgba(0xffffff1f))
        .text_color(rgba(0xffffffbd))
        .text_size(px(10.0))
        .font_semibold()
        .child(
            div()
                .w(px(6.0))
                .h(px(6.0))
                .rounded(px(3.0))
                .bg(rgba(save.dot)),
        )
        .child(save.label)
}

fn top_bar_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: rgba(0x050a1414).into(),
        offset: point(px(0.0), px(8.0)),
        blur_radius: px(30.0),
        spread_radius: px(0.0),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// 顶部工作区标题：绝对工作区根目录只显示末级项目名称。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   workspace root = `/Users/alan/Documents/mybook/multisoul-desktop`
    ///   路径层级数 = 5（Users / alan / Documents / mybook / multisoul-desktop）
    ///   project name = 最末级目录 `multisoul-desktop`
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 将完整绝对工作区路径传入顶部栏标题的展示边界。
    ///   2. 展示边界提取路径的末级目录名，供 GPUI 标题元素渲染。
    ///
    /// 预期结果：
    ///   - 正断言：标题为 `multisoul-desktop`，让用户识别当前项目。
    ///   - 负断言：标题不应保留 `/Users/alan/Documents/mybook/` 等绝对路径前缀。
    #[test]
    fn test_workspace_top_bar_title_uses_project_name_not_absolute_path() {
        let workspace_root = Path::new("/Users/alan/Documents/mybook/multisoul-desktop");
        let title = workspace_top_bar_title(Some(workspace_root));

        assert_eq!(
            title, "multisoul-desktop",
            "顶部栏应只显示工作区的末级项目名称"
        );
        assert_ne!(
            title, "/Users/alan/Documents/mybook/multisoul-desktop",
            "顶部栏不应泄露完整绝对工作区路径"
        );
    }
}
