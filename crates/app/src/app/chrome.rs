//! Workspace overlay chrome for the GPUI app shell.

use super::{
    MultisoulApp, WALLPAPER_IMAGE_OPACITY, WALLPAPER_SHADE_BOTTOM_ALPHA, WALLPAPER_SHADE_TOP_ALPHA,
};
use gpui::{
    div, img, linear_color_stop, linear_gradient, point, prelude::*, px, rgba, AnyView, App,
    BoxShadow, Context, Div, MouseButton, ObjectFit, Stateful, Window,
};
use ui::icons::{toolbar_icon, with_alpha, ToolbarIcon};
use ui::Theme;

const WORKSPACE_LEFT_RAIL_VISIBLE: bool = false;
const BOTTOM_VOICE_BAR_VISIBLE: bool = true;

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
            img(self.wallpaper_settings.image_source_for_render())
                .id("wallpaper")
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

        let mut chrome = div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .child(
                div()
                    .absolute()
                    .top(px(30.0))
                    .left(px(0.0))
                    .right(px(0.0))
                    .h(px(46.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .px(px(12.0))
                            .h(px(46.0))
                            .rounded(px(23.0))
                            .border_1()
                            .border_color(rgba(with_alpha(theme.edge_highlight, 0x34)))
                            .shadow(chrome_shadow(theme))
                            .bg(rgba(with_alpha(theme.panel, 0xdc)))
                            .text_color(rgba(theme.text_primary))
                            .text_size(px(12.0))
                            .child(
                                chrome_icon_button(
                                    theme,
                                    ToolbarIcon::Scroll,
                                    self.interaction.canvas_scroll_enabled,
                                    "toolbar-canvas-scroll",
                                    "Scroll mode",
                                )
                                .on_click(cx.listener(Self::on_toolbar_canvas_scroll)),
                            )
                            .child(chrome_divider(theme))
                            .child(
                                chrome_icon_button(
                                    theme,
                                    ToolbarIcon::Plus,
                                    self.interaction.new_menu_open,
                                    "toolbar-new-terminal",
                                    "New",
                                )
                                .on_click(cx.listener(Self::on_toolbar_toggle_new_menu)),
                            )
                            .child(chrome_divider(theme))
                            .child(
                                chrome_icon_button(
                                    theme,
                                    ToolbarIcon::Claude,
                                    false,
                                    "toolbar-new-claude-agent",
                                    "Claude",
                                )
                                .on_click(cx.listener(Self::on_toolbar_new_claude_agent)),
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
                            .child(chrome_divider(theme))
                            .child(
                                chrome_icon_button(
                                    theme,
                                    ToolbarIcon::Settings,
                                    false,
                                    "toolbar-settings",
                                    "Settings",
                                )
                                .on_click(cx.listener(Self::on_toolbar_settings)),
                            ),
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
            .when(self.interaction.new_menu_open, |chrome| {
                chrome.child(self.render_new_menu(cx))
            });

        if bottom_voice_bar_visible() {
            chrome = chrome.child(
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
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(Self::on_voice_bar_mouse_down),
                    )
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::on_voice_bar_mouse_up))
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
                            .child(self.voice_label()),
                    )
                    .child(
                        div()
                            .text_color(rgba(theme.accent_orange))
                            .child("........"),
                    ),
            );
        }

        chrome
    }
}

pub(super) fn chrome_shadow(theme: Theme) -> Vec<BoxShadow> {
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

fn bottom_voice_bar_visible() -> bool {
    BOTTOM_VOICE_BAR_VISIBLE
}

/// Toolbar icon button: a 30×30 stateful square holding one SVG icon with
/// hover/active/tooltip feedback. `active` swaps the rest tint for the accent
/// orange tint (mirror of the old text `chrome_button`).
///
/// Returns a `Stateful<Div>` so the caller only needs to chain `.on_click(...)`.
fn chrome_icon_button(
    theme: Theme,
    icon: ToolbarIcon,
    active: bool,
    id: &'static str,
    tooltip: &'static str,
) -> Stateful<Div> {
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
    let icon_color = if active {
        theme.accent_orange
    } else {
        theme.text_primary
    };

    div()
        .id(id)
        .w(px(30.0))
        .h(px(30.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(stroke))
        .bg(rgba(fill))
        .child(toolbar_icon(icon, icon_color, 16.0))
        .hover(move |s| s.bg(rgba(with_alpha(theme.edge_highlight, 0x14))))
        .active(move |s| s.bg(rgba(with_alpha(theme.edge_highlight, 0x28))))
        .tooltip(move |_, cx| tooltip_view(theme, tooltip, cx))
}

/// Vertical 1×20 separator between toolbar icon groups.
fn chrome_divider(theme: Theme) -> Div {
    div()
        .w(px(1.0))
        .h(px(20.0))
        .bg(rgba(with_alpha(theme.edge_highlight, 0x18)))
}

/// Build the hover tooltip `AnyView`. GPUI's `tooltip` closure must return an
/// `AnyView`; a stateless styled `Div` alone does not satisfy that, so we wrap
/// the pill in a tiny `Render` view entity via `App::new`.
fn tooltip_view(theme: Theme, text: &'static str, cx: &mut App) -> AnyView {
    cx.new(|_| TooltipView { theme, text }).into()
}

/// Stateless view backing [`tooltip_view`]: a small glass pill rendering `text`.
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

#[cfg(test)]
#[path = "../../tests/unit/app_chrome_tests.rs"]
mod tests;
