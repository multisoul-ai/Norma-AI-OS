//! ✚ 弹出菜单的渲染与点击处理：toggle 展开状态、4 个新建入口选中后
//! 关闭菜单并派发到既有 on_new_terminal / spawn_agent_card / on_open_preview_file。

use super::chrome::chrome_shadow;
use super::{MultisoulApp, NewBrowser, NewEgo, NewTerminal, OpenPreviewFile};
use crate::effects::dispatch_receipt::{COMMAND_DOCK_BOTTOM_PX, COMMAND_DOCK_HEIGHT_PX};
use canvas::{AgentProfile, CanvasRect};
use gpui::{
    div, prelude::*, px, rgba, App, ClickEvent, Context, Div, MouseButton, Stateful, Window,
};
use ui::icons::{toolbar_icon, with_alpha, ToolbarIcon};
use ui::Theme;

const NEW_MENU_WIDTH_PX: f32 = 180.0;
const NEW_MENU_TRIGGER_WIDTH_PX: f32 = 38.0;
// 12px separates the menu's rounded/shadowed surface from the rounded Dock trigger.
const NEW_MENU_TRIGGER_GAP_PX: f32 = 12.0;
// menu_left = -(menu_width - trigger_width) / 2 = -(180 - 38) / 2 = -71
const NEW_MENU_LEFT_OFFSET_PX: f32 = -(NEW_MENU_WIDTH_PX - NEW_MENU_TRIGGER_WIDTH_PX) / 2.0;
// GPUI 实际布局高度为 361.5px；固定到 362px 消除文字度量的半像素波动，也为遮挡几何提供单一真相。
const NEW_MENU_HEIGHT_PX: f32 = 362.0;
// Dock 底部 22px；52px Dock 内放置 38px 按钮，上下余量各 (52 - 38) / 2 = 7px。
// menu_bottom_from_window = 22 + 7 + trigger(38) + gap(12) = 79px。
const NEW_MENU_BOTTOM_FROM_WINDOW_PX: f32 = COMMAND_DOCK_BOTTOM_PX
    + (COMMAND_DOCK_HEIGHT_PX - NEW_MENU_TRIGGER_WIDTH_PX) / 2.0
    + NEW_MENU_TRIGGER_WIDTH_PX
    + NEW_MENU_TRIGGER_GAP_PX;

/// New 菜单的原生视图遮挡矩形。鼠标可在 38px 触发器内任意位置释放，
/// 因此水平两侧各外扩 19px，保证无论点中按钮哪一侧都能完整覆盖真实菜单。
pub(crate) fn new_menu_screen_rect(
    open: bool,
    activation_x: Option<f32>,
    win_h: f32,
) -> Option<CanvasRect> {
    if !open {
        return None;
    }
    let activation_x = activation_x?;
    let horizontal_guard = NEW_MENU_TRIGGER_WIDTH_PX / 2.0;
    Some(CanvasRect::new(
        activation_x - NEW_MENU_WIDTH_PX / 2.0 - horizontal_guard,
        win_h - NEW_MENU_BOTTOM_FROM_WINDOW_PX - NEW_MENU_HEIGHT_PX,
        NEW_MENU_WIDTH_PX + NEW_MENU_TRIGGER_WIDTH_PX,
        NEW_MENU_HEIGHT_PX,
    ))
}

fn new_menu_item_hover_fill(theme: Theme) -> u32 {
    with_alpha(theme.accent_blue, 0x2e)
}

fn new_menu_item_hover_border(theme: Theme) -> u32 {
    with_alpha(theme.accent_blue, 0x66)
}

fn new_menu_item_pressed_fill(theme: Theme) -> u32 {
    with_alpha(theme.accent_blue, 0x48)
}

impl MultisoulApp {
    #[cfg(feature = "e2e-test-support")]
    pub fn new_menu_visual_e2e_snapshot(&self) -> (bool, u32, u32, u32, u32) {
        (
            self.interaction.new_menu_open,
            self.theme.panel,
            new_menu_item_hover_fill(self.theme),
            new_menu_item_hover_border(self.theme),
            new_menu_item_pressed_fill(self.theme),
        )
    }

    /// ✚ 按钮点击：翻转菜单展开状态（按钮同时作为触发器与 toggle 指示）。
    pub(super) fn on_toolbar_toggle_new_menu(
        &mut self,
        event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let activation_x = match event {
            ClickEvent::Mouse(event) => f32::from(event.up.position.x),
            ClickEvent::Keyboard(event) => f32::from(event.bounds.center().x),
        };
        self.interaction.toggle_new_menu_at(activation_x);
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

    /// ✚ 弹出菜单：由 Dock 的 New 按钮容器承载，始终从触发按钮正上方浮出。
    pub(super) fn render_new_menu(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.theme;

        div()
            .id("new-menu")
            .debug_selector(|| "new-menu".to_string())
            .absolute()
            .bottom(px(NEW_MENU_TRIGGER_WIDTH_PX + NEW_MENU_TRIGGER_GAP_PX))
            .left(px(NEW_MENU_LEFT_OFFSET_PX))
            .flex()
            .flex_col()
            .gap_1()
            .p(px(6.0))
            .w(px(NEW_MENU_WIDTH_PX))
            .h(px(NEW_MENU_HEIGHT_PX))
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
            ))
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
        .border_1()
        .border_color(rgba(0xffffff00))
        .cursor_pointer()
        .child(toolbar_icon(icon, theme.text_primary, 16.0))
        .child(
            div()
                .text_color(rgba(theme.text_primary))
                .text_size(px(13.0))
                .child(label),
        )
        .hover(move |s| {
            s.bg(rgba(new_menu_item_hover_fill(theme)))
                .border_color(rgba(new_menu_item_hover_border(theme)))
        })
        .active(move |s| s.bg(rgba(new_menu_item_pressed_fill(theme))))
        .on_click(on_click)
}

#[cfg(test)]
#[path = "../../tests/unit/new_menu_tests.rs"]
mod new_menu_tests;
