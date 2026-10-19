//! 左栏「对话 / 运行概览」分段切换。
//!
//! 设计注记：
//! - 参考 apple-design、emil-design-eng、review-animations。
//! - 采纳：视图切换是偶尔发生的空间导航。进入概览自右 10px 滑入并淡入，回到对话自左滑入，
//!   200ms ease-out，只动水平位移与 opacity，并可从当前呈现值重定向。
//!   GPUI 0.2.2 的 Transformation 只在 Svg 上；div 用 overflow 裁切里的绝对
//!   left/right 做这 10px，标签和底部输入不参与位移。
//! - 采纳：减少动态效果时去掉位移，只淡入 120ms。
//! - 采纳：标签的悬停、按下和焦点即时变色，概览行点击不加动画。
//! - 拒绝弹簧和过冲。规格给定 200ms，这次不是带速度的拖拽。
//! - 拒绝 emil「键盘操作永不动画」中对这次切换本身的限制。⌘⇧E 与点击走同一视图切换，
//!   规格要求这次切换有方向感；按下反馈仍然即时。
//! - GPUI 0.2.2 的 overflow_hidden 只做矩形裁剪。抽屉圆角为 0；分段轨道自己绘制圆角。
//!   滑入层不是铺满圆角容器的不透明底。
//! - 分段控件用裸 div，而不是 ui::component::Button：需要 tablist 的 ←/→、
//!   选中与焦点同时可见，以及待处理角标。颜色取 `desktop_theme()`：浅色应用主题下
//!   抽屉仍是深色玻璃，状态色跟这块玻璃走，避免浅色 token 贴在深底上发灰。

use super::{GLASS_TEXT, GLASS_TEXT_SECONDARY};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, KeyDownEvent, Stateful, Window};
use runtime::desktop_drawers::{DesktopDrawerEvent, DesktopLeftView};
use ui::icons::with_alpha;

impl MultisoulApp {
    pub(super) fn render_left_drawer_tabs(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let selected = self.desktop_workspace.drawers.left_view;
        let pending = self.cached_workspace_pending_count();
        let chat_focus = self.left_chat_tab_focus(cx);
        let overview_focus = self.left_overview_tab_focus(cx);
        div()
            .id("desktop-left-view-tabs")
            .debug_selector(|| "desktop-left-view-tabs".to_string())
            .mx(px(16.0))
            .mt(px(12.0))
            .h(px(36.0))
            .flex()
            .items_center()
            .p(px(3.0))
            .gap(px(2.0))
            .rounded(px(10.0))
            .bg(rgba(with_alpha(theme.graphite, 0x88)))
            .border_1()
            .border_color(rgba(with_alpha(theme.edge_highlight, 0x40)))
            .child(self.left_view_tab(
                "desktop-left-tab-chat",
                ui::i18n::text("workspace.left_tab_chat"),
                None,
                selected == DesktopLeftView::Chat,
                chat_focus,
                cx,
            ))
            .child(self.left_view_tab(
                "desktop-left-tab-overview",
                ui::i18n::text("workspace.left_tab_overview"),
                (pending > 0).then_some(pending),
                selected == DesktopLeftView::Overview,
                overview_focus,
                cx,
            ))
    }

    fn left_view_tab(
        &self,
        id: &'static str,
        label: &'static str,
        pending: Option<usize>,
        selected: bool,
        focus: gpui::FocusHandle,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let view = if id.ends_with("overview") {
            DesktopLeftView::Overview
        } else {
            DesktopLeftView::Chat
        };
        let rest = if selected {
            with_alpha(theme.accent_blue, 0xc4)
        } else {
            0x00000000
        };
        let hover = if selected {
            with_alpha(theme.accent_blue, 0xe0)
        } else {
            with_alpha(theme.accent_blue, 0x55)
        };
        let pressed = if selected {
            with_alpha(theme.accent_blue, 0xff)
        } else {
            with_alpha(theme.accent_blue, 0x88)
        };
        div()
            .id(id)
            .debug_selector(move || id.to_string())
            .flex_1()
            .h(px(30.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(rgba(0x00000000))
            .cursor_pointer()
            .tab_index(0)
            .track_focus(&focus)
            .bg(rgba(rest))
            .text_size(px(12.0))
            .font_weight(if selected {
                gpui::FontWeight::SEMIBOLD
            } else {
                gpui::FontWeight::NORMAL
            })
            .text_color(rgba(if selected {
                GLASS_TEXT
            } else {
                GLASS_TEXT_SECONDARY
            }))
            .hover(move |style| style.bg(rgba(hover)))
            .active(move |style| style.bg(rgba(pressed)))
            .focus(|style| style.border_color(rgba(theme.focused_stroke)))
            .on_click(cx.listener(move |app, _, window, cx| {
                cx.stop_propagation();
                app.select_left_drawer_view(view, window, cx);
            }))
            .on_key_down(cx.listener(move |app, event: &KeyDownEvent, window, cx| {
                if app.handle_left_tab_key(view, event, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .child(label)
            .when_some(pending, |tab, count| {
                tab.child(
                    div()
                        .px(px(5.0))
                        .h(px(16.0))
                        .min_w(px(16.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(rgba(theme.accent_warm))
                        .text_size(px(11.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgba(0x1b1406ff))
                        .child(count.to_string()),
                )
            })
    }

    fn handle_left_tab_key(
        &mut self,
        current: DesktopLeftView,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.modifiers.modified() {
            return false;
        }
        let next = match event.keystroke.key.as_str() {
            "enter" | "space" => current,
            "right" | "left" => match (current, event.keystroke.key.as_str()) {
                (DesktopLeftView::Chat, "right") | (DesktopLeftView::Overview, "left") => {
                    if current == DesktopLeftView::Chat {
                        DesktopLeftView::Overview
                    } else {
                        DesktopLeftView::Chat
                    }
                }
                _ => return false,
            },
            "escape" if current == DesktopLeftView::Overview => {
                self.toggle_desktop_drawer(DesktopDrawerEvent::EscapeOverview, cx);
                self.focus_left_drawer_view(window, cx);
                return true;
            }
            _ => return false,
        };
        self.select_left_drawer_view(next, window, cx);
        true
    }

    pub(super) fn select_left_drawer_view(
        &mut self,
        view: DesktopLeftView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_desktop_drawer(DesktopDrawerEvent::SelectLeftView(view), cx);
        self.focus_left_drawer_view(window, cx);
    }

    pub(crate) fn toggle_run_overview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_desktop_drawer(DesktopDrawerEvent::ToggleRunOverview, cx);
        if matches!(self.screen, crate::app::AppScreen::DesktopWorkspace) {
            self.focus_left_drawer_view(window, cx);
        }
    }

    pub(super) fn focus_left_drawer_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let overview = self.desktop_workspace.drawers.left_view == DesktopLeftView::Overview;
        let handle = if overview {
            self.left_overview_tab_focus(cx)
        } else {
            self.left_chat_tab_focus(cx)
        };
        handle.focus(window);
    }

    pub(crate) fn dismiss_desktop_escape(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.key != "escape" || event.keystroke.modifiers.modified() {
            return false;
        }
        if self.desktop_workspace.switcher_open {
            self.dismiss_desktop_switcher(window, cx);
            cx.stop_propagation();
            return true;
        }
        if self.left_overview_holds_escape(window, cx) {
            self.toggle_desktop_drawer(DesktopDrawerEvent::EscapeOverview, cx);
            self.focus_left_drawer_view(window, cx);
            cx.stop_propagation();
            return true;
        }
        false
    }

    fn left_overview_holds_escape(&self, window: &Window, cx: &gpui::App) -> bool {
        if self.desktop_workspace.drawers.left_view != DesktopLeftView::Overview {
            return false;
        }
        self.desktop_workspace
            .left_overview_focus
            .as_ref()
            .is_some_and(|handle| handle.contains_focused(window, cx))
            || self
                .desktop_workspace
                .left_overview_tab_focus
                .as_ref()
                .is_some_and(|handle| handle.is_focused(window))
    }

    fn cached_workspace_pending_count(&self) -> usize {
        self.desktop_workspace
            .activity_view
            .as_ref()
            .map(|view| view.pending.len())
            .unwrap_or(0)
    }

    fn left_chat_tab_focus(&mut self, cx: &mut Context<Self>) -> gpui::FocusHandle {
        self.desktop_workspace
            .left_chat_tab_focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone()
    }

    fn left_overview_tab_focus(&mut self, cx: &mut Context<Self>) -> gpui::FocusHandle {
        self.desktop_workspace
            .left_overview_tab_focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone()
    }

    pub(super) fn left_overview_focus(&mut self, cx: &mut Context<Self>) -> gpui::FocusHandle {
        self.desktop_workspace
            .left_overview_focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone()
    }
}
