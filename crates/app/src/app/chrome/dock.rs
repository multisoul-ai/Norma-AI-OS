//! Command Dock rendering and its local GPUI elements.

use super::super::MultisoulApp;
use super::bottom_voice_bar_visible;
use crate::effects::dispatch_receipt::{COMMAND_DOCK_BOTTOM_PX, COMMAND_DOCK_HEIGHT_PX};
use gpui::{
    div, point, prelude::*, px, rgba, AnyView, App, BoxShadow, Context, Div, MouseButton, Stateful,
    Window,
};
use ui::icons::{toolbar_icon, with_alpha, ToolbarIcon};
use ui::Theme;

impl MultisoulApp {
    pub(super) fn render_command_dock(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.theme;
        let canvas_mode_icon = if self.interaction.canvas_scroll_enabled {
            ToolbarIcon::Scroll
        } else {
            ToolbarIcon::Cursor
        };
        let canvas_mode_tooltip = if self.interaction.canvas_scroll_enabled {
            "Scroll mode"
        } else {
            "Select mode"
        };
        let new_menu = self
            .interaction
            .new_menu_open
            .then(|| self.render_new_menu(cx));
        div()
            .absolute()
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(COMMAND_DOCK_BOTTOM_PX))
            .flex()
            .justify_center()
            .child(
                div()
                    .id("workspace-command-dock")
                    .debug_selector(|| "workspace-command-dock".to_string())
                    .h(px(COMMAND_DOCK_HEIGHT_PX))
                    .flex()
                    .items_center()
                    .gap(px(3.0))
                    .p(px(6.0))
                    .rounded(px(18.0))
                    .border_1()
                    .border_color(rgba(theme.dock_line))
                    .shadow(command_dock_shadow())
                    .bg(rgba(theme.dock))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .relative()
                            .w(px(38.0))
                            .h(px(38.0))
                            .child(
                                chrome_primary_icon_button(
                                    theme,
                                    ToolbarIcon::Plus,
                                    self.interaction.new_menu_open,
                                    "toolbar-new-terminal",
                                    "New",
                                )
                                .on_click(cx.listener(Self::on_toolbar_toggle_new_menu)),
                            )
                            .when_some(new_menu, |trigger, menu| trigger.child(menu)),
                    )
                    .child(
                        chrome_icon_button(
                            theme,
                            ToolbarIcon::Terminal,
                            false,
                            "toolbar-terminal",
                            "Terminal",
                        )
                        .on_click(cx.listener(Self::on_new_menu_terminal)),
                    )
                    .child(
                        chrome_icon_button(
                            theme,
                            ToolbarIcon::Codex,
                            false,
                            "toolbar-new-codex-agent",
                            "Codex",
                        )
                        .on_click(cx.listener(Self::on_toolbar_new_codex_agent)),
                    )
                    .child(
                        chrome_icon_button(
                            theme,
                            ToolbarIcon::Browser,
                            false,
                            "toolbar-new-browser",
                            "Browser",
                        )
                        .on_click(cx.listener(Self::on_new_menu_browser)),
                    )
                    .child(
                        chrome_icon_button(
                            theme,
                            ToolbarIcon::Preview,
                            false,
                            "toolbar-open-preview",
                            "Preview",
                        )
                        .on_click(cx.listener(Self::on_new_menu_preview)),
                    )
                    .child(chrome_divider(theme))
                    .child(
                        chrome_icon_button(
                            theme,
                            canvas_mode_icon,
                            true,
                            "toolbar-canvas-scroll",
                            canvas_mode_tooltip,
                        )
                        .on_click(cx.listener(Self::on_toolbar_canvas_scroll)),
                    )
                    .child(
                        chrome_icon_button(
                            theme,
                            ToolbarIcon::Layout,
                            self.interaction.layout_menu_open,
                            "toolbar-layout",
                            "Layout",
                        )
                        .on_click(cx.listener(Self::on_toolbar_toggle_layout_menu)),
                    )
                    .child(
                        chrome_icon_button(
                            theme,
                            ToolbarIcon::Settings,
                            false,
                            "toolbar-settings",
                            "Settings",
                        )
                        .on_click(cx.listener(Self::on_toolbar_settings)),
                    )
                    .child(chrome_divider(theme))
                    .when(bottom_voice_bar_visible(), |dock| {
                        dock.child(self.render_voice_bar(window, cx))
                    }),
            )
    }
}

fn command_dock_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: rgba(0x04091247).into(),
        offset: point(px(0.0), px(20.0)),
        blur_radius: px(55.0),
        spread_radius: px(0.0),
    }]
}

/// Toolbar icon button: a stateful square holding one SVG icon with hover,
/// active, and tooltip feedback. Returns a `Stateful<Div>` for click chaining.
fn chrome_icon_button(
    theme: Theme,
    icon: ToolbarIcon,
    active: bool,
    id: &'static str,
    tooltip: &'static str,
) -> Stateful<Div> {
    let fill = if active {
        with_alpha(theme.accent_cyan, 0x28)
    } else {
        0xffffff00
    };
    let icon_color = if active { 0xffffffff } else { 0xffffffd6 };
    let hover_fill = if active {
        with_alpha(theme.accent_cyan, 0x40)
    } else {
        0xffffff24
    };
    let pressed_fill = if active {
        with_alpha(theme.accent_cyan, 0x54)
    } else {
        0xffffff33
    };

    div()
        .id(id)
        .debug_selector(move || id.to_string())
        .w(px(38.0))
        .h(px(38.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(12.0))
        .bg(rgba(fill))
        .shadow(command_button_shadow(active))
        .child(toolbar_icon(icon, icon_color, 16.0))
        .hover(move |style| style.bg(rgba(hover_fill)))
        .active(move |style| style.bg(rgba(pressed_fill)))
        .tooltip(move |_, cx| tooltip_view(theme, tooltip, cx))
}

/// Dock 的 New 是唯一的主操作：持续以低饱和 cyan 锚定创建组，打开菜单时再提高强调。
fn chrome_primary_icon_button(
    theme: Theme,
    icon: ToolbarIcon,
    active: bool,
    id: &'static str,
    tooltip: &'static str,
) -> Stateful<Div> {
    let fill = if active {
        with_alpha(theme.accent_cyan, 0x70)
    } else {
        with_alpha(theme.accent_cyan, 0x4d)
    };
    let hover_fill = with_alpha(theme.accent_cyan, 0x60);
    let pressed_fill = with_alpha(theme.accent_cyan, 0x7d);

    div()
        .id(id)
        .debug_selector(move || id.to_string())
        .w(px(38.0))
        .h(px(38.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(12.0))
        .bg(rgba(fill))
        .shadow(command_button_shadow(active))
        .child(toolbar_icon(icon, 0xffffffff, 16.0))
        .hover(move |style| style.bg(rgba(hover_fill)))
        .active(move |style| style.bg(rgba(pressed_fill)))
        .tooltip(move |_, cx| tooltip_view(theme, tooltip, cx))
}

/// Vertical 1×22 separator between toolbar icon groups.
fn chrome_divider(theme: Theme) -> Div {
    div()
        .w(px(1.0))
        .h(px(22.0))
        .bg(rgba(with_alpha(theme.dock_line, 0x38)))
}

fn command_button_shadow(active: bool) -> Vec<BoxShadow> {
    if !active {
        return Vec::new();
    }
    vec![BoxShadow {
        color: rgba(0x04091233).into(),
        offset: point(px(0.0), px(7.0)),
        blur_radius: px(18.0),
        spread_radius: px(0.0),
    }]
}

/// Build the tooltip `AnyView` required by GPUI's `tooltip` closure.
fn tooltip_view(theme: Theme, text: &'static str, cx: &mut App) -> AnyView {
    cx.new(|_| TooltipView { theme, text }).into()
}

/// Stateless view backing [`tooltip_view`].
struct TooltipView {
    theme: Theme,
    text: &'static str,
}

impl Render for TooltipView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(6.0))
            .bg(rgba(with_alpha(self.theme.panel, 0xf0)))
            .border_1()
            .border_color(rgba(with_alpha(self.theme.edge_highlight, 0x40)))
            .text_color(rgba(self.theme.text_primary))
            .text_size(px(11.0))
            .child(self.text)
    }
}
