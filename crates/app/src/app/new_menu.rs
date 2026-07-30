//! ✚ 弹出菜单的渲染与点击处理：toggle 展开状态、4 个新建入口选中后
//! 关闭菜单并派发到既有 on_new_terminal / spawn_agent_card / on_open_preview_file。

use super::chrome::chrome_shadow;
use super::{MultisoulApp, NewBrowser, NewEgo, NewTerminal, OpenPreviewFile};
use canvas::AgentProfile;
use gpui::{
    div, prelude::*, px, rgba, App, ClickEvent, Context, Div, MouseButton, Stateful, Window,
};
use ui::icons::{toolbar_icon, with_alpha, ToolbarIcon};
use ui::Theme;

impl MultisoulApp {
    /// ✚ 按钮点击：翻转菜单展开状态（按钮同时作为触发器与 toggle 指示）。
    pub(super) fn on_toolbar_toggle_new_menu(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.interaction.toggle_new_menu();
        cx.notify();
    }

    pub(super) fn on_new_menu_terminal(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.interaction.close_new_menu();
        self.on_new_terminal(&NewTerminal, window, cx);
        cx.notify();
    }

    pub(super) fn on_new_menu_claude(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.interaction.close_new_menu();
        self.spawn_agent_card(AgentProfile::Claude, window, cx);
        cx.notify();
    }

    pub(super) fn on_new_menu_codex(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.interaction.close_new_menu();
        self.spawn_agent_card(AgentProfile::Codex, window, cx);
        cx.notify();
    }

    pub(super) fn on_new_menu_opencode(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.interaction.close_new_menu();
        self.spawn_agent_card(AgentProfile::OpenCode, window, cx);
        cx.notify();
    }

    pub(super) fn on_new_menu_cursor(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.interaction.close_new_menu();
        self.spawn_agent_card(AgentProfile::Cursor, window, cx);
        cx.notify();
    }

    pub(super) fn on_new_menu_grok(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.interaction.close_new_menu();
        self.spawn_agent_card(AgentProfile::Grok, window, cx);
        cx.notify();
    }

    pub(super) fn on_new_menu_preview(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.interaction.close_new_menu();
        self.on_open_preview_file(&OpenPreviewFile, window, cx);
        cx.notify();
    }

    pub(super) fn on_new_menu_browser(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.interaction.close_new_menu();
        self.on_new_browser(&NewBrowser, window, cx);
        cx.notify();
    }

    pub(super) fn on_new_menu_ego(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.interaction.close_new_menu();
        self.on_new_ego(&NewEgo, window, cx);
        cx.notify();
    }

    /// ✚ 弹出菜单：在底部 Command Dock 上方居中悬浮，列出所有新建入口。
    pub(super) fn render_new_menu(&self, cx: &mut Context<Self>) -> Div {
        let theme = self.theme;

        div()
            .absolute()
            .bottom(px(82.0))
            .left(px(0.0))
            .right(px(0.0))
            .flex()
            .flex_row()
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p(px(6.0))
                    .w(px(180.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.edge_highlight, 0x34)))
                    .shadow(chrome_shadow(theme))
                    .bg(rgba(with_alpha(theme.panel, 0xf2)))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(new_menu_item(
                        theme,
                        ToolbarIcon::Terminal,
                        "Terminal",
                        "new-menu-terminal",
                        cx.listener(Self::on_new_menu_terminal),
                    ))
                    .child(new_menu_item(
                        theme,
                        ToolbarIcon::Claude,
                        "Claude",
                        "new-menu-claude",
                        cx.listener(Self::on_new_menu_claude),
                    ))
                    .child(new_menu_item(
                        theme,
                        ToolbarIcon::Codex,
                        "Codex",
                        "new-menu-codex",
                        cx.listener(Self::on_new_menu_codex),
                    ))
                    .child(new_menu_item(
                        theme,
                        ToolbarIcon::Cursor,
                        "Cursor",
                        "new-menu-cursor",
                        cx.listener(Self::on_new_menu_cursor),
                    ))
                    .child(new_menu_item(
                        theme,
                        ToolbarIcon::Grok,
                        "Grok",
                        "new-menu-grok",
                        cx.listener(Self::on_new_menu_grok),
                    ))
                    .child(new_menu_item(
                        theme,
                        ToolbarIcon::OpenCode,
                        "OpenCode",
                        "new-menu-opencode",
                        cx.listener(Self::on_new_menu_opencode),
                    ))
                    .child(new_menu_item(
                        theme,
                        ToolbarIcon::Preview,
                        "Preview",
                        "new-menu-preview",
                        cx.listener(Self::on_new_menu_preview),
                    ))
                    .child(new_menu_item(
                        theme,
                        ToolbarIcon::Browser,
                        "Browser",
                        "new-menu-browser",
                        cx.listener(Self::on_new_menu_browser),
                    ))
                    .child(new_menu_item(
                        theme,
                        ToolbarIcon::Browser,
                        "Ego",
                        "new-menu-ego",
                        cx.listener(Self::on_new_menu_ego),
                    )),
            )
    }
}

/// 单个 ✚ 菜单项：图标 + 文案 + hover 高亮 + 点击回调。
fn new_menu_item(
    theme: Theme,
    icon: ToolbarIcon,
    label: &'static str,
    id: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.to_string())
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .px(px(10.0))
        .py(px(6.0))
        .rounded(px(8.0))
        .child(toolbar_icon(icon, theme.text_primary, 16.0))
        .child(
            div()
                .text_color(rgba(theme.text_primary))
                .text_size(px(13.0))
                .child(label),
        )
        .hover(move |s| s.bg(rgba(with_alpha(theme.edge_highlight, 0x18))))
        .on_click(on_click)
}
