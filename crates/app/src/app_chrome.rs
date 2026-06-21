//! Workspace overlay chrome for the GPUI app shell.

use super::{
    MultisoulApp, WALLPAPER_IMAGE_OPACITY, WALLPAPER_SHADE_BOTTOM_ALPHA, WALLPAPER_SHADE_TOP_ALPHA,
};
use gpui::{
    div, img, linear_color_stop, linear_gradient, point, prelude::*, px, rgba, BoxShadow, Context,
    Div, ObjectFit, Window,
};
use ui::Theme;

const WORKSPACE_LEFT_RAIL_VISIBLE: bool = false;
const TOP_TOOLBAR_WIDTH: f32 = 340.0;

impl MultisoulApp {
    pub(super) fn render_workspace_backdrop(&self) -> Div {
        let theme = self.theme;
        let mut backdrop = div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .overflow_hidden()
            .bg(linear_gradient(
                150.0,
                linear_color_stop(rgba(theme.canvas_from), 0.0),
                linear_color_stop(rgba(theme.graphite), 1.0),
            ));

        backdrop = backdrop.child(
            img(self.wallpaper.image_source())
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .size_full()
                .object_fit(ObjectFit::Cover)
                .opacity(WALLPAPER_IMAGE_OPACITY),
        );

        backdrop
            .child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .right(px(0.0))
                    .bottom(px(0.0))
                    .bg(linear_gradient(
                        180.0,
                        linear_color_stop(
                            rgba(with_alpha(theme.graphite, WALLPAPER_SHADE_TOP_ALPHA)),
                            0.0,
                        ),
                        linear_color_stop(
                            rgba(with_alpha(theme.graphite, WALLPAPER_SHADE_BOTTOM_ALPHA)),
                            1.0,
                        ),
                    )),
            )
            .child(
                div()
                    .absolute()
                    .left(px(-160.0))
                    .top(px(54.0))
                    .w(px(520.0))
                    .h(px(520.0))
                    .rounded(px(260.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.accent_cyan, 0x24))),
            )
            .child(
                div()
                    .absolute()
                    .right(px(-180.0))
                    .bottom(px(-120.0))
                    .w(px(620.0))
                    .h(px(620.0))
                    .rounded(px(310.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.accent_orange, 0x2e))),
            )
            .child(
                div()
                    .absolute()
                    .left(px(90.0))
                    .right(px(90.0))
                    .bottom(px(84.0))
                    .h(px(1.0))
                    .bg(rgba(with_alpha(theme.accent_cyan, 0x18))),
            )
    }

    pub(super) fn render_workspace_chrome(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.theme;
        let win = window.viewport_size();
        let win_w = f32::from(win.width);
        let bottom_w = (win_w - 64.0).clamp(260.0, 360.0);
        let bottom_left = (win_w - bottom_w) * 0.5;

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .child(
                div()
                    .absolute()
                    .top(px(30.0))
                    .left(px((win_w - TOP_TOOLBAR_WIDTH) * 0.5))
                    .w(px(TOP_TOOLBAR_WIDTH))
                    .h(px(46.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .px(px(12.0))
                    .rounded(px(23.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.edge_highlight, 0x34)))
                    .shadow(chrome_shadow(theme))
                    .bg(rgba(with_alpha(theme.panel, 0xdc)))
                    .text_color(rgba(theme.text_primary))
                    .text_size(px(12.0))
                    .child(chrome_button(theme, "v", false))
                    .child(div().px(px(8.0)).child("CANVAS"))
                    .child(
                        chrome_button(theme, "+", true)
                            .id("toolbar-new-terminal")
                            .on_click(cx.listener(Self::on_toolbar_new_terminal)),
                    )
                    .child(
                        chrome_button(theme, "Claude Code", false)
                            .id("toolbar-new-claude-agent")
                            .on_click(cx.listener(Self::on_toolbar_new_claude_agent)),
                    )
                    .child(
                        chrome_button(theme, "Codex", false)
                            .id("toolbar-new-codex-agent")
                            .on_click(cx.listener(Self::on_toolbar_new_codex_agent)),
                    ),
            )
            .when(workspace_left_rail_visible(), |chrome| {
                chrome.child(
                    div()
                        .absolute()
                        .left(px(24.0))
                        .top(px(178.0))
                        .w(px(46.0))
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_2()
                        .p(px(8.0))
                        .rounded(px(23.0))
                        .border_1()
                        .border_color(rgba(with_alpha(theme.edge_highlight, 0x2f)))
                        .shadow(chrome_shadow(theme))
                        .bg(rgba(with_alpha(theme.panel, 0xd0)))
                        .child(chrome_button(theme, ">", true))
                        .child(chrome_button(theme, "H", false))
                        .child(chrome_button(theme, "[]", false))
                        .child(chrome_button(theme, "<>", false))
                        .child(chrome_button(theme, "O", false))
                        .child(chrome_button(theme, "T", false))
                        .child(chrome_button(theme, "AI", false))
                        .child(chrome_button(theme, "{}", false)),
                )
            })
            .child(
                div()
                    .absolute()
                    .left(px(bottom_left))
                    .bottom(px(22.0))
                    .w(px(bottom_w))
                    .h(px(42.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px(px(18.0))
                    .rounded(px(21.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.edge_highlight, 0x2d)))
                    .shadow(chrome_shadow(theme))
                    .bg(rgba(with_alpha(theme.panel, 0xe5)))
                    .text_color(rgba(theme.text_primary))
                    .text_size(px(13.0))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(18.0))
                                    .h(px(18.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(rgba(theme.accent_orange))
                                    .child("|||"),
                            )
                            .child("Listening..."),
                    )
                    .child(
                        div()
                            .text_color(rgba(theme.accent_orange))
                            .child("........"),
                    ),
            )
    }
}

fn chrome_shadow(theme: Theme) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: rgba(theme.shadow_color).into(),
        offset: point(px(0.0), px(18.0)),
        blur_radius: px(36.0),
        spread_radius: px(-18.0),
    }]
}

fn chrome_button(theme: Theme, label: &'static str, active: bool) -> Div {
    let fill = if active {
        with_alpha(theme.accent_orange, 0x36)
    } else {
        with_alpha(theme.edge_highlight, 0x08)
    };
    let stroke = if active {
        with_alpha(theme.accent_orange, 0x78)
    } else {
        with_alpha(theme.edge_highlight, 0x20)
    };
    let text = if active {
        theme.accent_orange
    } else {
        theme.text_primary
    };

    div()
        .w(px(chrome_button_width(label)))
        .h(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(stroke))
        .bg(rgba(fill))
        .text_color(rgba(text))
        .text_size(px(11.0))
        .child(label)
}

fn chrome_button_width(label: &str) -> f32 {
    let text_width = label.chars().count() as f32 * 6.4;
    (text_width + 18.0).max(28.0).ceil()
}

fn workspace_left_rail_visible() -> bool {
    WORKSPACE_LEFT_RAIL_VISIBLE
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 左侧菜单列表：临时隐藏期间不应渲染 workspace 左侧纵向菜单。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   workspace_left_rail_visible() = false（临时隐藏开关；后续恢复时只需改回 true）
    ///   左侧菜单位置 = left 24px, top 178px, width 46px（当前被开关包住，不进入渲染树）
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 读取 workspace_left_rail_visible()
    ///   2. false → render_workspace_chrome 跳过左侧菜单列表分支
    ///
    /// 预期结果：
    ///   - 正断言：左侧菜单开关为 false，表示列表应被隐藏
    ///   - 负断言：左侧菜单开关不应为 true，否则用户仍会看到暂时隐藏的菜单列表
    #[test]
    fn test_workspace_left_rail_is_temporarily_hidden() {
        let left_rail_visible = workspace_left_rail_visible();

        assert!(
            !left_rail_visible,
            "左侧菜单列表临时隐藏期间 workspace_left_rail_visible() 应返回 false"
        );
        assert_ne!(
            left_rail_visible, true,
            "左侧菜单列表不应继续标记为可见，否则渲染树仍会挂载该列表"
        );
    }

    /// 顶部 agent 菜单：截图中的旧缩写按钮应替换为完整名称，并移除末尾两个占位按钮。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   source = include_str!("app_chrome.rs")（检查当前 chrome 渲染源码）
    ///   render_source = source 中 #[cfg(test)] 前的生产渲染区，避免测试注释误命中
    ///   expected_1 = "Claude" + " Code"（CL 应展开为完整产品名）
    ///   expected_2 = "Co" + "dex"（CX 应展开为完整产品名）
    ///   forbidden_1 = "\"C" + "L\""（截图里的旧 Claude 缩写 literal）
    ///   forbidden_2 = "\"C" + "X\""（截图里的旧 Codex 缩写 literal）
    ///   forbidden_3 = "toolbar" + "-reflow"（截图里 CX 后面的 [] 按钮 id）
    ///   forbidden_4 = "chrome_button(theme, \"*\", false)"（截图里最后一个星号占位按钮）
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 读取 app_chrome.rs 的生产渲染源码
    ///   2. 确认 agent 按钮包含新的完整文案
    ///   3. 搜索旧缩写 literal 与截图里的两个末尾无用按钮
    ///
    /// 预期结果：
    ///   - 正断言：Claude agent 顶部按钮显示为 Claude Code
    ///   - 正断言：Codex agent 顶部按钮显示为 Codex
    ///   - 负断言：CL/CX 旧缩写不应继续出现在生产渲染源码中
    ///   - 负断言：[] reflow 按钮和 * 占位按钮不应继续挂载
    #[test]
    fn test_top_toolbar_agent_labels_are_expanded_and_trailing_buttons_removed() {
        let source = include_str!("app_chrome.rs");
        let render_source = source.split("#[cfg(test)]").next().unwrap_or(source);
        let claude_label = format!("{}{}", "Claude", " Code");
        let codex_label = format!("{}{}", "Co", "dex");
        let legacy_claude_literal = format!("\"{}{}\"", "C", "L");
        let legacy_codex_literal = format!("\"{}{}\"", "C", "X");
        let trailing_reflow_id = format!("{}{}", "toolbar", "-reflow");
        let trailing_star_button = format!("{}{}", "chrome_button(theme, \"", "*\", false)");

        assert!(
            render_source.contains(&claude_label),
            "顶部 Claude agent 按钮应显示 Claude Code，不能继续只显示 CL"
        );
        assert!(
            render_source.contains(&codex_label),
            "顶部 Codex agent 按钮应显示 Codex，不能继续只显示 CX"
        );
        assert!(
            !render_source.contains(&legacy_claude_literal),
            "顶部 Claude agent 按钮不应继续渲染 CL 缩写 literal"
        );
        assert!(
            !render_source.contains(&legacy_codex_literal),
            "顶部 Codex agent 按钮不应继续渲染 CX 缩写 literal"
        );
        assert!(
            !render_source.contains(&trailing_reflow_id),
            "顶部菜单不应继续挂载截图中 CX 后面的 [] reflow 按钮"
        );
        assert!(
            !render_source.contains(&trailing_star_button),
            "顶部菜单不应继续挂载截图中最后的星号占位按钮"
        );
    }

    /// 右上角反馈入口：workspace chrome 不应再挂载反馈按钮。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   source = include_str!("app_chrome.rs")（检查当前 chrome 源码）
    ///   render_source = source 中 #[cfg(test)] 前的生产渲染区，避免测试注释误命中
    ///   toolbar id = "toolbar" + "-new-terminal"（应继续存在，证明顶部工具栏没有被误删）
    ///   forbidden label = "Feed" + "back"（用户要求删除的按钮文案）
    ///   forbidden id = "workspace-" + "feedback"（旧按钮命中 id）
    ///   forbidden handler = "on_" + "feedback" + "_click"（旧按钮点击处理入口）
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 读取 app_chrome.rs 的生产渲染源码
    ///   2. 确认 toolbar-new-terminal 仍存在 → chrome 基本结构仍在
    ///   3. 搜索旧反馈按钮的文案、id、点击 handler
    ///
    /// 预期结果：
    ///   - 正断言：顶部新终端按钮仍存在
    ///   - 负断言：旧反馈文案、id、handler 不应继续出现在 chrome 渲染源码中
    #[test]
    fn test_workspace_chrome_omits_feedback_entry() {
        let source = include_str!("app_chrome.rs");
        let render_source = source.split("#[cfg(test)]").next().unwrap_or(source);
        let toolbar_new_terminal_id = concat!("toolbar", "-new-terminal");
        let feedback_label = format!("{}{}", "Feed", "back");
        let feedback_id = format!("{}{}", "workspace-", "feedback");
        let feedback_handler = format!("{}{}{}", "on_", "feedback", "_click");

        assert!(
            render_source.contains(toolbar_new_terminal_id),
            "workspace chrome 应保留顶部新终端按钮，避免测试因 chrome 被整体删除而误通过"
        );
        assert!(
            !render_source.contains(&feedback_label),
            "workspace chrome 不应继续渲染右上角反馈按钮文案"
        );
        assert!(
            !render_source.contains(&feedback_id),
            "workspace chrome 不应继续挂载旧反馈按钮 id"
        );
        assert!(
            !render_source.contains(&feedback_handler),
            "workspace chrome 不应继续绑定旧反馈按钮点击 handler"
        );
    }
}
