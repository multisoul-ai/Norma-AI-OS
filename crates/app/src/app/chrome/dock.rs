//! Command Dock rendering and its local GPUI elements.

use super::super::command_dock::DOCK_RADIUS;
use super::super::{AppScreen, MultisoulApp};
use super::bottom_voice_bar_visible;
use crate::effects::dispatch_receipt::{COMMAND_DOCK_BOTTOM_PX, COMMAND_DOCK_HEIGHT_PX};
use gpui::{
    div, point, prelude::*, px, rgba, AnyElement, AnyView, App, BoxShadow, Context, Div,
    MouseButton, Stateful, Window,
};
use ui::icons::{with_alpha, ToolbarIcon};
use ui::Theme;

impl MultisoulApp {
    pub(crate) fn render_command_dock(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        project_switcher: Option<AnyElement>,
    ) -> Div {
        let theme = self.theme;
        if !matches!(self.screen, AppScreen::Canvas) {
            return self.render_desktop_overview_dock(window, cx, project_switcher);
        }
        let input = self.render_command_input(window, cx);
        let suggestions = self.render_command_suggestions(window, cx);
        let top_radius = self.dock_top_radius();
        div()
            .absolute()
            .left(px(18.0))
            .right(px(18.0))
            .bottom(px(COMMAND_DOCK_BOTTOM_PX))
            .flex()
            .justify_center()
            .child(
                div()
                    .id("workspace-command-dock")
                    .debug_selector(|| "workspace-command-dock".into())
                    .min_w(px(0.0))
                    .max_w(px(840.0))
                    .h(px(COMMAND_DOCK_HEIGHT_PX))
                    .flex()
                    .items_center()
                    .gap(px(3.0))
                    .p(px(6.0))
                    .relative()
                    .rounded_tl(px(top_radius))
                    .rounded_tr(px(top_radius))
                    .rounded_bl(px(DOCK_RADIUS))
                    .rounded_br(px(DOCK_RADIUS))
                    .border_1()
                    .border_color(rgba(theme.dock_line))
                    .shadow(command_dock_shadow())
                    .bg(rgba(theme.dock))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .children(suggestions)
                    .child(
                        chrome_icon_button(
                            theme,
                            "icons/command-workspace.svg",
                            false,
                            "toolbar-workspace",
                            ui::i18n::text("chrome.workspaces"),
                        )
                        .on_click(cx.listener(|app, _, window, cx| {
                            app.hide_command_suggestions();
                            app.open_desktop_switcher(window, cx);
                        })),
                    )
                    .child(
                        chrome_icon_button(
                            theme,
                            ToolbarIcon::Layout.asset_path(),
                            self.interaction.layout_menu_open,
                            "toolbar-layout",
                            ui::i18n::text("chrome.layout"),
                        )
                        .on_click(cx.listener(|app, event, window, cx| {
                            app.hide_command_suggestions();
                            app.on_toolbar_toggle_layout_menu(event, window, cx);
                        })),
                    )
                    .child(chrome_divider(theme))
                    .child(input)
                    .child(chrome_divider(theme))
                    .when(bottom_voice_bar_visible(), |dock| {
                        dock.child(self.render_voice_bar(window, cx))
                    })
                    .children(project_switcher)
                    .child(
                        chrome_icon_button(
                            theme,
                            ToolbarIcon::Settings.asset_path(),
                            self.settings_panel.open,
                            "toolbar-settings",
                            ui::i18n::text("common.settings"),
                        )
                        .on_click(cx.listener(|app, event, window, cx| {
                            app.hide_command_suggestions();
                            app.on_toolbar_settings(event, window, cx);
                        })),
                    ),
            )
    }

    fn render_desktop_overview_dock(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        project_switcher: Option<AnyElement>,
    ) -> Div {
        let theme = self.theme;
        let input = self.render_command_input(window, cx);
        let suggestions = self.render_command_suggestions(window, cx);
        let top_radius = self.dock_top_radius();
        div()
            .absolute()
            .left(px(18.0))
            .right(px(18.0))
            .bottom(px(COMMAND_DOCK_BOTTOM_PX))
            .flex()
            .justify_center()
            .child(
                div()
                    .id("workspace-command-dock")
                    .debug_selector(|| "workspace-command-dock".to_string())
                    .min_w(px(0.0))
                    .max_w(px(840.0))
                    .h(px(COMMAND_DOCK_HEIGHT_PX))
                    .flex()
                    .items_center()
                    .gap(px(3.0))
                    .p(px(6.0))
                    .relative()
                    .rounded_tl(px(top_radius))
                    .rounded_tr(px(top_radius))
                    .rounded_bl(px(DOCK_RADIUS))
                    .rounded_br(px(DOCK_RADIUS))
                    .border_1()
                    .border_color(rgba(theme.dock_line))
                    .shadow(command_dock_shadow())
                    .bg(rgba(theme.dock))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .children(suggestions)
                    .child(
                        chrome_icon_button(
                            theme,
                            ToolbarIcon::Plus.asset_path(),
                            false,
                            "toolbar-new-terminal",
                            ui::i18n::text("shortcuts.add_project"),
                        )
                        .on_click(cx.listener(|app, _, window, cx| {
                            app.open_desktop_add_project(window, cx)
                        })),
                    )
                    .child(chrome_divider(theme))
                    .child(input)
                    .child(chrome_divider(theme))
                    .children(project_switcher)
                    .child(
                        chrome_icon_button(
                            theme,
                            "icons/command-workspace.svg",
                            false,
                            "toolbar-switch-workspace",
                            ui::i18n::text("chrome.switch_workspace_k"),
                        )
                        .on_click(
                            cx.listener(|app, _, window, cx| app.open_desktop_switcher(window, cx)),
                        ),
                    )
                    .child(
                        chrome_icon_button(
                            theme,
                            ToolbarIcon::Settings.asset_path(),
                            false,
                            "toolbar-settings",
                            ui::i18n::text("common.settings"),
                        )
                        .on_click(cx.listener(|app, _, _, cx| app.toggle_settings_panel(cx))),
                    ),
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
    icon: &'static str,
    active: bool,
    id: &'static str,
    tooltip: &'static str,
) -> Stateful<Div> {
    let fill = if active {
        with_alpha(theme.accent_cyan, 0x28)
    } else {
        0xffffff00
    };
    let icon_color = dock_icon_color(theme, active);
    let hover_fill = if active {
        with_alpha(theme.accent_cyan, 0x40)
    } else {
        with_alpha(
            if theme.is_light() {
                theme.text_primary
            } else {
                0xffffffff
            },
            0x24,
        )
    };
    let pressed_fill = if active {
        with_alpha(theme.accent_cyan, 0x54)
    } else {
        with_alpha(
            if theme.is_light() {
                theme.text_primary
            } else {
                0xffffffff
            },
            0x33,
        )
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
        .flex_none()
        .tab_index(0)
        .cursor_pointer()
        .focus(move |style| style.bg(rgba(hover_fill)))
        .child(
            gpui::svg()
                .path(icon)
                .text_color(rgba(icon_color))
                .w(px(16.0))
                .h(px(16.0)),
        )
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

fn dock_icon_color(theme: Theme, active: bool) -> u32 {
    if theme.is_light() {
        if active {
            theme.accent
        } else {
            theme.text_secondary
        }
    } else if active {
        0xffffffff
    } else {
        0xffffffd6
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/dock_ink_tests.rs"]
mod dock_ink_tests;
