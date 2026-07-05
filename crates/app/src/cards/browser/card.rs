//! Browser card renderer.

use crate::app::{BrowserCommand, BrowserCommandKind};
use crate::cards::chrome::CardChrome;
use crate::cards::{Card, CardBodyContext};
use canvas::{CanvasNode, CanvasNodeKind, CanvasRect, NodeType};
use gpui::{div, prelude::*, px, rgba, AnyElement, MouseButton};
use runtime::{NodeResize, Runtime};
use ui::icons::with_alpha;

const BROWSER_PADDING_PX: f32 = 10.0;
pub(crate) const BROWSER_TOOLBAR_H_PX: f32 = 34.0;

pub struct BrowserCard;

pub static CARD: BrowserCard = BrowserCard;

impl Card for BrowserCard {
    fn ty(&self) -> NodeType {
        NodeType::Browser
    }

    fn chrome(&self, node: &CanvasNode, _runtime: &Runtime) -> CardChrome {
        CardChrome {
            title: node.kind.display_title(),
            refreshable: false,
            task_badge: None,
        }
    }

    fn body(&self, node: &CanvasNode, context: CardBodyContext<'_>) -> AnyElement {
        render_browser_body(node, context)
    }

    fn resize(&self, _bounds: CanvasRect, _zoom: f32) -> Option<NodeResize> {
        None
    }

    fn wheel_scroll_mode(&self) -> crate::cards::CardWheelScrollMode {
        crate::cards::CardWheelScrollMode::NativeElement
    }
}

fn render_browser_body(node: &CanvasNode, context: CardBodyContext<'_>) -> AnyElement {
    let theme = context.theme;
    let zoom = context.zoom;
    let node_id = node.id;
    let url = context
        .browser_address_draft
        .map(str::to_string)
        .unwrap_or_else(|| browser_url(node));
    let editing = context.browser_address_draft.is_some();
    let app_focus_handle = context.app_focus_handle.clone();

    div()
        .id(("browser-card", node_id))
        .flex()
        .flex_col()
        .size_full()
        .bg(rgba(with_alpha(theme.panel_high, 0x72)))
        .rounded_b(px(context.bottom_radius_px))
        .overflow_hidden()
        .child(
            div()
                .id(("browser-toolbar", node.id))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.0 * zoom))
                .h(px(BROWSER_TOOLBAR_H_PX * zoom))
                .px(px(BROWSER_PADDING_PX * zoom))
                .bg(rgba(with_alpha(theme.panel, 0xd0)))
                .child(toolbar_button(
                    node_id,
                    BrowserCommandKind::Back,
                    "‹",
                    theme.text_tertiary,
                    zoom,
                ))
                .child(toolbar_button(
                    node_id,
                    BrowserCommandKind::Forward,
                    "›",
                    theme.text_tertiary,
                    zoom,
                ))
                .child(toolbar_button(
                    node_id,
                    BrowserCommandKind::Reload,
                    "R",
                    theme.text_tertiary,
                    zoom,
                ))
                .child(
                    div()
                        .id(("browser-address", node_id))
                        .flex_1()
                        .min_w(px(0.0))
                        .h(px(22.0 * zoom))
                        .flex()
                        .items_center()
                        .px(px(8.0 * zoom))
                        .rounded(px(6.0 * zoom))
                        .bg(rgba(with_alpha(
                            if editing {
                                theme.accent_cyan
                            } else {
                                theme.term_bg
                            },
                            if editing { 0x24 } else { 0x72 },
                        )))
                        .text_size(px((12.0 * zoom).max(9.0)))
                        .text_color(rgba(theme.text_secondary))
                        .overflow_hidden()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .min_w(px(0.0))
                                .overflow_hidden()
                                .child(div().min_w(px(0.0)).overflow_hidden().truncate().child(url))
                                .when(editing, |this| {
                                    this.child(
                                        div()
                                            .id(("browser-address-caret", node_id))
                                            .flex_none()
                                            .ml(px(1.0 * zoom))
                                            .w(px((1.0 * zoom).max(1.0)))
                                            .h(px((14.0 * zoom).clamp(10.0, 16.0)))
                                            .bg(rgba(theme.accent_cyan)),
                                    )
                                }),
                        )
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            app_focus_handle.focus(window);
                            cx.stop_propagation();
                        })
                        .on_mouse_up(MouseButton::Left, move |_, window, cx| {
                            window.dispatch_action(
                                Box::new(BrowserCommand {
                                    node_id,
                                    command: BrowserCommandKind::EditAddress,
                                }),
                                cx,
                            );
                        }),
                ),
        )
        .child(
            div()
                .id(("browser-native-surface", node_id))
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px((12.0 * zoom).max(9.0)))
                .text_color(rgba(theme.text_tertiary))
                .child("Loading browser..."),
        )
        .into_any_element()
}

fn toolbar_button(
    node_id: canvas::NodeId,
    command: BrowserCommandKind,
    label: &'static str,
    color: u32,
    zoom: f32,
) -> gpui::Div {
    let size = (22.0 * zoom).clamp(18.0, 28.0);
    div()
        .w(px(size))
        .h(px(size))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0 * zoom))
        .text_size(px((13.0 * zoom).clamp(10.0, 14.0)))
        .text_color(rgba(color))
        .child(label)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_up(MouseButton::Left, move |_, window, cx| {
            window.dispatch_action(Box::new(BrowserCommand { node_id, command }), cx);
        })
}

fn browser_url(node: &CanvasNode) -> String {
    match &node.kind {
        CanvasNodeKind::Browser(spec) => spec.url.clone(),
        _ => String::new(),
    }
}
