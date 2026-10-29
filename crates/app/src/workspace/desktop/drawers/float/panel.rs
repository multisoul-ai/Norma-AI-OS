//! 悬浮面板的标题栏、图钉和列表。拖动与焦点落点在 drag.rs。

use super::super::super::activity_cache::overview_clock_ms;
use super::super::super::overview_rows::overview_subtitle;

use super::{overview_panel_fill, prefers_reduced_transparency, PANEL_RADIUS};
use crate::app::MultisoulApp;
use gpui::{
    div, point, prelude::*, px, rgba, svg, BoxShadow, Context, Div, KeyDownEvent, MouseButton,
    MouseDownEvent, Stateful, Window,
};
use runtime::desktop_drawers::{place_overview, DesktopDrawerEvent, OverviewPlacement};
use ui::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use ui::icons::with_alpha;

impl MultisoulApp {
    pub(in crate::workspace::desktop) fn render_floating_overview(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        if !self.desktop_workspace.drawers.overview_floating {
            return None;
        }
        self.ensure_idea_input(window, cx);
        let canvas = self.overview_canvas(window);
        let place = place_overview(self.desktop_workspace.drawers.overview_offset, canvas);
        let layout = self.desktop_assistant_layout(window);
        let opacity = self.desktop_workspace.overview_float_fade.opacity();
        let panel = self.render_overview_panel(place, layout.left_space, opacity, window, cx);
        Some(panel)
    }

    pub(in crate::workspace::desktop) fn render_overview_drawer_header(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.overview_title_bar(false, cx)
    }

    fn render_overview_panel(
        &mut self,
        place: OverviewPlacement,
        canvas_left: f32,
        opacity: f32,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let fill = overview_panel_fill(theme, prefers_reduced_transparency());
        let focus = self.overview_float_focus(cx);
        div()
            .id("desktop-run-overview-float")
            .debug_selector(|| "desktop-run-overview-float".to_string())
            .absolute()
            .left(px(canvas_left + place.left))
            .top(px(place.top))
            .w(px(place.width))
            .max_h(px(place.max_height))
            .opacity(opacity)
            .flex()
            .flex_col()
            .rounded(px(PANEL_RADIUS))
            .border_1()
            .border_color(rgba(theme.node_stroke))
            .bg(rgba(fill))
            .text_color(rgba(theme.text_primary))
            .shadow(vec![BoxShadow {
                color: rgba(crate::workspace::desktop::surface_theme::shadow_color(
                    theme, 0xbb,
                ))
                .into(),
                offset: point(px(0.0), px(22.0)),
                blur_radius: px(52.0),
                spread_radius: px(0.0),
            }])
            .tab_index(0)
            .track_focus(&focus)
            .focus(|style| style.border_color(rgba(theme.focused_stroke)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_key_down(cx.listener(|app, event: &KeyDownEvent, window, cx| {
                if app.handle_run_overview_key(
                    event.keystroke.key.as_str(),
                    event.keystroke.modifiers.modified(),
                    window,
                    cx,
                ) {
                    cx.stop_propagation();
                    return;
                }
                if event.keystroke.key == "escape"
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                    && !event.keystroke.modifiers.alt
                {
                    app.focus_run_status(window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(self.overview_title_bar(true, cx))
            .child(self.render_run_overview(cx))
    }

    fn overview_title_bar(&mut self, floating: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let name = self
            .desktop_workspace
            .active()
            .map(|workspace| workspace.name.clone())
            .unwrap_or_default();
        let subtitle = overview_subtitle(
            &name,
            self.desktop_workspace.activity_updated_at_ms,
            overview_clock_ms(),
        );
        div()
            .id(if floating {
                "desktop-overview-float-header"
            } else {
                "desktop-overview-drawer-header"
            })
            .debug_selector(move || {
                if floating {
                    "desktop-overview-float-header"
                } else {
                    "desktop-overview-drawer-header"
                }
                .to_string()
            })
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(if floating { 16.0 } else { 12.0 }))
            .pt(px(if floating { 12.0 } else { 8.0 }))
            .pb(px(if floating { 10.0 } else { 6.0 }))
            .when(floating, |header| {
                header
                    .border_b_1()
                    .border_color(rgba(with_alpha(theme.text_primary, 0x1c)))
                    .cursor_grab()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|app, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            app.begin_overview_drag(event, cx);
                        }),
                    )
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgba(theme.text_primary))
                            .child(ui::i18n::text("workspace.left_tab_overview")),
                    )
                    .when(!subtitle.is_empty(), |title| {
                        title.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgba(theme.text_secondary))
                                .child(subtitle),
                        )
                    }),
            )
            .when(floating, |header| {
                header.child(self.overview_move_handle(cx))
            })
            .child(self.overview_pin_button(floating, theme, cx))
    }

    fn overview_pin_button(
        &mut self,
        floating: bool,
        theme: ui::Theme,
        cx: &mut Context<Self>,
    ) -> Button {
        let id = if floating {
            "desktop-overview-float-pin"
        } else {
            "desktop-overview-pin"
        };
        let tooltip = if floating {
            ui::i18n::text("workspace.overview_pin_dock")
        } else {
            ui::i18n::text("workspace.overview_pin_float")
        };
        let toggled = floating;
        Button::new(id)
            .debug_selector(move || id.to_string())
            .w(px(28.0))
            .h(px(28.0))
            .p(px(0.0))
            .rounded(px(7.0))
            .tooltip(tooltip)
            .child(
                svg()
                    .path("icons/pin.svg")
                    .text_color(rgba(if toggled {
                        theme.text_primary
                    } else {
                        theme.accent
                    }))
                    .w(px(14.0))
                    .h(px(14.0)),
            )
            .custom(pin_variant(theme, toggled, cx))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|app, _, _, cx| {
                cx.stop_propagation();
                app.toggle_desktop_drawer(DesktopDrawerEvent::ToggleOverviewPin, cx);
            }))
    }

    fn overview_move_handle(&mut self, cx: &mut Context<Self>) -> Button {
        let theme = self.desktop_theme();
        let focus = self.overview_move_focus(cx);
        Button::new("desktop-overview-move")
            .debug_selector(|| "desktop-overview-move".to_string())
            .w(px(28.0))
            .h(px(28.0))
            .p(px(0.0))
            .rounded(px(7.0))
            .cursor_grab()
            .tab_index(0)
            .track_focus(&focus)
            .tooltip(ui::i18n::text("workspace.overview_move"))
            .child(
                svg()
                    .path("icons/grip.svg")
                    .text_color(rgba(theme.text_secondary))
                    .w(px(14.0))
                    .h(px(14.0)),
            )
            .custom(pin_variant(theme, false, cx))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|app, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    app.begin_overview_drag(event, cx);
                }),
            )
            .on_key_down(cx.listener(|app, event: &KeyDownEvent, window, cx| {
                if app.nudge_overview_from_key(event, window, cx) {
                    cx.stop_propagation();
                }
            }))
    }
}

fn pin_variant(theme: ui::Theme, toggled: bool, cx: &gpui::App) -> ButtonCustomVariant {
    let rest = if toggled {
        with_alpha(theme.accent_blue, 0x55)
    } else {
        0x00000000
    };
    let hover = if toggled {
        with_alpha(theme.accent_blue, 0x77)
    } else {
        with_alpha(theme.accent, 0x24)
    };
    let pressed = if toggled {
        with_alpha(theme.accent_blue, 0x99)
    } else {
        with_alpha(theme.accent, 0x3a)
    };
    ButtonCustomVariant::new(cx)
        .color(rgba(rest).into())
        .foreground(rgba(theme.text_primary).into())
        .border(
            rgba(if toggled {
                with_alpha(theme.accent_blue, 0x8c)
            } else {
                0x00000000
            })
            .into(),
        )
        .hover(rgba(hover).into())
        .active(rgba(pressed).into())
}
