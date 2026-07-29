//! Browser card renderer.

use crate::app::browser::BrowserAuthUiState;
use crate::app::ego::{EgoCardCommand, EgoCardCommandKind};
use crate::app::{BrowserCommand, BrowserCommandKind};
use crate::cards::chrome::{CardChrome, CardChromeTaskBadge, CardChromeTaskTone};
use crate::cards::{Card, CardBodyContext};
use canvas::{BrowserSurfaceKind, CanvasNode, CanvasNodeKind, CanvasRect, EgoCardPhase, NodeType};
use gpui::{div, prelude::*, px, rgba, AnyElement, MouseButton};
use runtime::{NodeResize, Runtime};
use ui::component::button::{Button, ButtonRounded, ButtonVariants};
use ui::{icons::with_alpha, Theme};

const BROWSER_PADDING_PX: f32 = 10.0;
pub(crate) const BROWSER_TOOLBAR_H_PX: f32 = 34.0;

pub struct BrowserCard;

pub static CARD: BrowserCard = BrowserCard;

impl Card for BrowserCard {
    fn ty(&self) -> NodeType {
        NodeType::Browser
    }

    fn chrome(&self, node: &CanvasNode, runtime: &Runtime) -> CardChrome {
        if is_ego_node(node) {
            let (label, tone) = runtime
                .ego_task_state(node.id)
                .map(|state| ego_badge(state.phase))
                .unwrap_or(("Ego 已断开", CardChromeTaskTone::Error));
            return CardChrome {
                title: "Ego".to_string(),
                refreshable: false,
                task_badge: Some(CardChromeTaskBadge {
                    label: label.to_string(),
                    tone,
                }),
            };
        }
        CardChrome {
            title: node.kind.display_title(),
            refreshable: false,
            task_badge: None,
        }
    }

    fn body(&self, node: &CanvasNode, context: CardBodyContext<'_>) -> AnyElement {
        if is_ego_node(node) {
            render_ego_body(node, context)
        } else {
            render_browser_body(node, context)
        }
    }

    fn resize(&self, _bounds: CanvasRect, _zoom: f32) -> Option<NodeResize> {
        None
    }

    fn wheel_scroll_mode(&self) -> crate::cards::CardWheelScrollMode {
        crate::cards::CardWheelScrollMode::NativeElement
    }
}

fn render_ego_body(node: &CanvasNode, context: CardBodyContext<'_>) -> AnyElement {
    let theme = context.theme;
    let zoom = context.zoom;
    let node_id = node.id;
    let state = context.runtime.ego_task_state(node_id);
    let phase = state
        .as_ref()
        .map(|state| state.phase)
        .unwrap_or(EgoCardPhase::Disconnected);
    let message = state
        .and_then(|state| state.message)
        .unwrap_or_else(|| ego_status_text(phase).to_string());

    div()
        .id(("ego-card", node_id))
        .debug_selector(|| "ego-card".to_string())
        .flex()
        .flex_col()
        .size_full()
        .rounded_b(px(context.bottom_radius_px))
        .overflow_hidden()
        .bg(rgba(with_alpha(theme.panel_high, 0x72)))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(px(8.0 * zoom))
                .px(px(12.0 * zoom))
                .h(px(36.0 * zoom))
                .bg(rgba(with_alpha(theme.panel, 0xd8)))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.0 * zoom))
                        .text_size(px((11.0 * zoom).max(9.0)))
                        .text_color(rgba(theme.text_secondary))
                        .child("外部窗口模式"),
                )
                .child(ego_toolbar_action(
                    node_id,
                    EgoCardCommandKind::Open,
                    "打开 Ego",
                    zoom,
                )),
        )
        .child(
            div()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(12.0 * zoom))
                .px(px(24.0 * zoom))
                .child(
                    div()
                        .text_size(px((21.0 * zoom).clamp(16.0, 26.0)))
                        .text_color(rgba(theme.text_primary))
                        .child("Ego Lite"),
                )
                .child(
                    div()
                        .text_size(px((12.0 * zoom).max(9.0)))
                        .text_color(rgba(theme.text_secondary))
                        .child(ego_status_text(phase)),
                )
                .child(
                    div()
                        .max_w(px(460.0 * zoom))
                        .text_size(px((11.0 * zoom).max(9.0)))
                        .text_color(rgba(theme.text_tertiary))
                        .child(message),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(8.0 * zoom))
                        .when(phase == EgoCardPhase::AwaitingUser, |actions| {
                            actions
                                .child(ego_primary_action(
                                    node_id,
                                    EgoCardCommandKind::SetMaster,
                                    "设为 Master",
                                    zoom,
                                ))
                                .child(ego_primary_action(
                                    node_id,
                                    EgoCardCommandKind::Takeover,
                                    "接管",
                                    zoom,
                                ))
                        })
                        .when(phase == EgoCardPhase::UserControlling, |actions| {
                            actions.child(ego_primary_action(
                                node_id,
                                EgoCardCommandKind::ReturnToAgent,
                                "交还 AI",
                                zoom,
                            ))
                        }),
                ),
        )
        .into_any_element()
}

fn ego_toolbar_action(
    node_id: canvas::NodeId,
    command: EgoCardCommandKind,
    label: &'static str,
    zoom: f32,
) -> impl IntoElement {
    ego_action_button(node_id, command, label, zoom, "ego-open-button", false)
}

fn ego_primary_action(
    node_id: canvas::NodeId,
    command: EgoCardCommandKind,
    label: &'static str,
    zoom: f32,
) -> impl IntoElement {
    let id = match command {
        EgoCardCommandKind::SetMaster => "ego-set-master-button",
        EgoCardCommandKind::Takeover => "ego-takeover-button",
        EgoCardCommandKind::ReturnToAgent => "ego-return-button",
        EgoCardCommandKind::Open => "ego-open-button",
    };
    ego_action_button(node_id, command, label, zoom, id, true)
}

fn ego_action_button(
    node_id: canvas::NodeId,
    command: EgoCardCommandKind,
    label: &'static str,
    zoom: f32,
    id: &'static str,
    primary: bool,
) -> impl IntoElement {
    let button = Button::new((id, node_id))
        .debug_selector(move || id.to_string())
        .label(label)
        .tooltip(label)
        .rounded(ButtonRounded::Size(px(7.0 * zoom)))
        .px(px(10.0 * zoom))
        .h(px((28.0 * zoom).clamp(24.0, 34.0)))
        .text_size(px((11.0 * zoom).max(9.0)));
    let button = if primary {
        button.primary()
    } else {
        button.ghost()
    };

    div()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(button.on_click(move |_, window, cx| {
            window.dispatch_action(Box::new(EgoCardCommand { node_id, command }), cx);
        }))
}

fn ego_badge(phase: EgoCardPhase) -> (&'static str, CardChromeTaskTone) {
    match phase {
        EgoCardPhase::Creating => ("正在创建", CardChromeTaskTone::Active),
        EgoCardPhase::ReadyForUser => ("可由用户操作", CardChromeTaskTone::Muted),
        EgoCardPhase::PreparingAgent => ("Agent 正在准备", CardChromeTaskTone::Active),
        EgoCardPhase::AgentWorking => ("Agent 正在浏览", CardChromeTaskTone::Active),
        EgoCardPhase::AwaitingUser => ("请求你接管", CardChromeTaskTone::Warning),
        EgoCardPhase::UserControlling => ("你正在控制", CardChromeTaskTone::Warning),
        EgoCardPhase::ResumeRequested => ("等待 Agent 恢复", CardChromeTaskTone::Warning),
        EgoCardPhase::Completed => ("任务已完成", CardChromeTaskTone::Muted),
        EgoCardPhase::Failed => ("任务失败", CardChromeTaskTone::Error),
        EgoCardPhase::Disconnected => ("Ego 已断开", CardChromeTaskTone::Error),
    }
}

fn ego_status_text(phase: EgoCardPhase) -> &'static str {
    match phase {
        EgoCardPhase::Creating => "正在创建 Ego 空间",
        EgoCardPhase::ReadyForUser => "可由用户操作",
        EgoCardPhase::PreparingAgent => "Agent 正在准备",
        EgoCardPhase::AgentWorking => "Agent 正在浏览",
        EgoCardPhase::AwaitingUser => "Agent 请求你接管",
        EgoCardPhase::UserControlling => "你正在控制 Ego",
        EgoCardPhase::ResumeRequested => "已交还，等待 Agent 恢复",
        EgoCardPhase::Completed => "任务已完成",
        EgoCardPhase::Failed => "任务失败",
        EgoCardPhase::Disconnected => "Ego 已断开",
    }
}

fn is_ego_node(node: &CanvasNode) -> bool {
    matches!(
        &node.kind,
        CanvasNodeKind::Browser(spec)
            if matches!(&spec.surface, BrowserSurfaceKind::EgoLite(_))
    )
}

fn render_browser_body(node: &CanvasNode, context: CardBodyContext<'_>) -> AnyElement {
    let theme = context.theme;
    let zoom = context.zoom;
    let node_id = node.id;
    let url = browser_url(node);
    let browser_address_input = context.browser_address_input;
    let editing = browser_address_input.is_some();

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
                    theme,
                    zoom,
                ))
                .child(toolbar_button(
                    node_id,
                    BrowserCommandKind::Forward,
                    "›",
                    theme,
                    zoom,
                ))
                .child(toolbar_button(
                    node_id,
                    BrowserCommandKind::Reload,
                    "R",
                    theme,
                    zoom,
                ))
                .child(toolbar_button(
                    node_id,
                    BrowserCommandKind::ImportChromeAuth,
                    chrome_auth_label(context.browser_auth_state),
                    theme,
                    zoom,
                ))
                .child(
                    div()
                        .id(("browser-address", node_id))
                        .debug_selector(|| "browser-address".to_string())
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
                        .when_some(browser_address_input, |this, browser_address_input| {
                            this.child(browser_address_input)
                        })
                        .when(!editing, |this| {
                            this.child(div().min_w(px(0.0)).overflow_hidden().truncate().child(url))
                                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
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
                                })
                        }),
                ),
        )
        .child(
            div()
                .id(("browser-native-surface", node_id))
                .debug_selector(|| "browser-native-surface".to_string())
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
    theme: Theme,
    zoom: f32,
) -> impl IntoElement {
    let size = (22.0 * zoom).clamp(18.0, 28.0);
    let width = match command {
        BrowserCommandKind::ImportChromeAuth => (70.0 * zoom).clamp(56.0, 84.0),
        _ => size,
    };
    let control_id = match command {
        BrowserCommandKind::Back => "browser-back",
        BrowserCommandKind::Forward => "browser-forward",
        BrowserCommandKind::Reload => "browser-reload",
        BrowserCommandKind::EditAddress => "browser-edit-address",
        BrowserCommandKind::ImportChromeAuth => "browser-chrome-auth",
    };
    div()
        .id((control_id, node_id))
        .debug_selector(move || control_id.to_string())
        .w(px(width))
        .h(px(size))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0 * zoom))
        .border_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x16)))
        .bg(rgba(with_alpha(theme.edge_highlight, 0x08)))
        .text_size(px((13.0 * zoom).clamp(10.0, 14.0)))
        .text_color(rgba(theme.text_tertiary))
        .cursor_pointer()
        .child(label)
        .hover(move |style| {
            style
                .bg(rgba(with_alpha(theme.edge_highlight, 0x18)))
                .border_color(rgba(with_alpha(theme.edge_highlight, 0x38)))
                .text_color(rgba(theme.text_secondary))
        })
        .active(move |style| {
            style
                .bg(rgba(with_alpha(theme.accent_cyan, 0x2e)))
                .border_color(rgba(with_alpha(theme.accent_cyan, 0x78)))
                .text_color(rgba(theme.accent_cyan))
        })
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_up(MouseButton::Left, move |_, window, cx| {
            window.dispatch_action(Box::new(BrowserCommand { node_id, command }), cx);
        })
}

fn chrome_auth_label(state: BrowserAuthUiState) -> &'static str {
    match state {
        BrowserAuthUiState::Idle => "Chrome",
        BrowserAuthUiState::Waiting => "Sync…",
        BrowserAuthUiState::Ready => "Chrome ✓",
        BrowserAuthUiState::Failed => "Retry",
    }
}

fn browser_url(node: &CanvasNode) -> String {
    match &node.kind {
        CanvasNodeKind::Browser(spec) => spec.url.clone(),
        _ => String::new(),
    }
}
