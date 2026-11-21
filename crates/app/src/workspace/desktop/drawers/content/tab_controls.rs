use super::chrome::{compact_tab_controls, COMPACT_TOOLS_HEIGHT};
use super::chrome_format::{
    content_tab_title, reader_chrome_colors, READER_CHROME_HEIGHT, READER_TOOLBAR_HEIGHT,
};
use crate::app::MultisoulApp;
use gpui::Focusable;
use gpui::{
    anchored, deferred, div, point, prelude::*, px, rgba, svg, Context, Corner, DismissEvent,
    Entity, FocusHandle, Pixels, Point, ScrollHandle, Subscription, Window,
};
use runtime::desktop_drawers::ContentTabCloseScope;
use storage::DesktopContentTab;
use ui::component::{
    button::{Button, ButtonVariants},
    menu::{PopupMenu, PopupMenuItem},
    IconName, Selectable,
};

#[derive(Default)]
pub(crate) struct ContentTabControls {
    pub(super) strip: ScrollHandle,
    pub(super) closing: Vec<String>,
    pub(super) middle_pressed: Option<String>,
    reveal: Option<String>,
    menu: Option<TabMenu>,
    pub(super) search: Option<super::tab_search::TabSearch>,
}

struct TabMenu {
    target: String,
    anchor: Point<Pixels>,
    view: Entity<PopupMenu>,
    _dismiss: Subscription,
}

impl MultisoulApp {
    pub(crate) fn dismiss_content_tab_controls(&mut self) {
        self.content_tab_controls.menu = None;
        self.content_tab_controls.search = None;
    }

    pub(super) fn request_content_tab_reveal(&mut self, id: &str) {
        self.content_tab_controls.reveal = Some(id.to_owned());
    }

    pub(super) fn reveal_committed_content_tab(&mut self, cx: &mut Context<Self>) {
        let state = &mut self.desktop_workspace;
        if let Some(id) = self.content_tab_controls.reveal.take() {
            if let Some(index) = state
                .content_tabs
                .iter()
                .position(|tab| tab.selected && tab.id == id)
            {
                self.content_tab_controls.strip.scroll_to_item(index);
                cx.notify();
            }
        }
    }

    pub(super) fn content_tab_overlay_open(&self) -> bool {
        let state = &self.desktop_workspace;
        self.content_tab_controls.search.is_some()
            || self
                .content_tab_controls
                .menu
                .as_ref()
                .is_some_and(|menu| state.content_tabs.iter().any(|tab| tab.id == menu.target))
    }

    pub(super) fn tab_control_button(
        &self,
        id: &'static str,
        icon: IconName,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> Button {
        Button::new(id)
            .debug_selector(move || id.into())
            .icon(icon)
            .w(px(32.0))
            .h(px(32.0))
            .p(px(0.0))
            .flex_none()
            .rounded(px(9.0))
            .custom(self.reader_control_variant(reader_chrome_colors(self.desktop_theme()), cx))
            .tooltip(ui::i18n::text(label))
    }

    pub(in crate::workspace::desktop::drawers::content) fn render_content_tab_actions(
        &self,
        tab: &DesktopContentTab,
        width: f32,
        right_inset: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let id = tab.id.clone();
        let open = self
            .content_tab_controls
            .menu
            .as_ref()
            .is_some_and(|menu| menu.target == id);
        let button = Button::new("desktop-content-tab-actions")
            .debug_selector(|| "desktop-content-tab-actions".into())
            .w(px(32.0))
            .h(px(32.0))
            .p(px(0.0))
            .flex_none()
            .rounded(px(9.0))
            .child(div().text_size(px(17.0)).child("⋯"))
            .custom(self.reader_control_variant(reader_chrome_colors(self.desktop_theme()), cx))
            .tooltip(ui::i18n::text("drawers.tab_actions"));
        let extra = if compact_tab_controls(width) {
            COMPACT_TOOLS_HEIGHT
        } else {
            0.0
        };
        let y = super::super::TOP + READER_CHROME_HEIGHT + extra + READER_TOOLBAR_HEIGHT;
        let anchor = point(window.viewport_size().width - px(right_inset + 12.0), px(y));
        let focus = window
            .use_keyed_state("desktop-content-actions-focus", cx, |_, cx| {
                cx.focus_handle().tab_stop(true)
            })
            .read(cx)
            .clone();
        let key_focus = focus.clone();
        let key_id = id.clone();
        let focus_color = rgba(self.desktop_theme().focused_stroke);
        div()
            .id("desktop-content-tab-actions-trigger")
            .flex_none()
            .track_focus(&focus)
            .rounded(px(9.0))
            .border_1()
            .border_color(rgba(0x00000000))
            .focus(move |style| style.border_color(focus_color))
            .on_key_down(
                cx.listener(move |app, event: &gpui::KeyDownEvent, window, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space" | "down") {
                        app.open_content_tab_menu(&key_id, anchor, key_focus.clone(), window, cx);
                        cx.stop_propagation();
                    }
                }),
            )
            .child(
                button
                    .selected(open)
                    .tab_stop(false)
                    .when(width >= 330.0, |button| {
                        button
                            .w(px(96.0))
                            .px(px(8.0))
                            .label(ui::i18n::text("drawers.tab_actions"))
                    })
                    .on_click(cx.listener(move |app, _, window, cx| {
                        app.open_content_tab_menu(&id, anchor, focus.clone(), window, cx);
                    })),
            )
    }

    pub(super) fn open_content_tab_menu(
        &mut self,
        id: &str,
        anchor: Point<Pixels>,
        return_focus: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tabs = &self.desktop_workspace.content_tabs;
        let Some(index) = tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        let title = content_tab_title(&tabs[index]);
        let count = tabs.len();
        let target = id.to_owned();
        self.dismiss_content_tab_controls();
        let app = cx.weak_entity();
        let scopes = [
            (ContentTabCloseScope::Current, "drawers.close_tab", true),
            (
                ContentTabCloseScope::Others,
                "drawers.close_other_tabs",
                count > 1,
            ),
            (
                ContentTabCloseScope::Right,
                "drawers.close_right_tabs",
                index + 1 < count,
            ),
            (ContentTabCloseScope::All, "drawers.close_all_tabs", true),
        ];
        let can_reload = self.content_tab_can_reload(id);
        let view = PopupMenu::build(window, cx, |mut menu, _, _| {
            menu = menu
                .action_context(return_focus)
                .min_w(px(186.0))
                .max_w(px(265.0))
                .label(title)
                .separator();
            for (scope, label, enabled) in scopes {
                let app = app.clone();
                let id = target.clone();
                menu = menu.item(
                    PopupMenuItem::new(ui::i18n::text(label))
                        .disabled(!enabled)
                        .on_click(move |_, _, cx| {
                            let _ = app.update(cx, |app, cx| {
                                app.close_desktop_content_tab_scope(&id, scope, cx)
                            });
                        }),
                );
            }
            let id = target.clone();
            menu.separator().item(
                PopupMenuItem::new(ui::i18n::text("drawers.reload_tab"))
                    .disabled(!can_reload)
                    .on_click(move |_, window, cx| {
                        let _ = app.update(cx, |app, cx| {
                            app.refresh_desktop_content_tab(&id, window, cx)
                        });
                    }),
            )
        });
        let dismiss = cx.subscribe(&view, |app, _, _: &DismissEvent, cx| {
            app.content_tab_controls.menu = None;
            cx.notify();
        });
        view.focus_handle(cx).focus(window);
        self.content_tab_controls.menu = Some(TabMenu {
            target,
            anchor,
            view,
            _dismiss: dismiss,
        });
        self.hide_desktop_content_browsers();
        cx.notify();
    }

    pub(super) fn render_content_tab_menu(&self) -> Option<gpui::AnyElement> {
        let state = &self.desktop_workspace;
        let menu = self.content_tab_controls.menu.as_ref()?;
        if !state.content_tabs.iter().any(|tab| tab.id == menu.target) {
            return None;
        }
        Some(
            deferred(
                anchored()
                    .anchor(Corner::TopRight)
                    .position(menu.anchor)
                    .snap_to_window()
                    .child(
                        div()
                            .id("desktop-content-action-menu")
                            .debug_selector(|| "desktop-content-action-menu".into())
                            .occlude()
                            .child(menu.view.clone()),
                    ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    pub(super) fn render_content_refresh_button(&self, id: &str, cx: &mut Context<Self>) -> Button {
        use ui::component::{spinner::Spinner, Disableable, Sizable};
        let enabled = self.content_tab_can_reload(id);
        let loading = self.content_tab_is_refreshing(id);
        let id = id.to_owned();
        let theme = self.desktop_theme();
        Button::new("desktop-content-refresh")
            .debug_selector(|| "desktop-content-refresh".into())
            .w(px(32.0))
            .h(px(32.0))
            .p(px(0.0))
            .flex_none()
            .rounded(px(9.0))
            .custom(self.reader_control_variant(reader_chrome_colors(theme), cx))
            .tooltip(ui::i18n::text("drawers.refresh_tab"))
            .when(loading && !self.motion.reduce_motion, |button| {
                button.child(
                    Spinner::new()
                        .with_size(px(16.0))
                        .color(rgba(theme.accent).into()),
                )
            })
            .when(!loading || self.motion.reduce_motion, |button| {
                button.child(
                    svg()
                        .path("icons/refresh.svg")
                        .w(px(16.0))
                        .h(px(16.0))
                        .text_color(rgba(theme.text_primary)),
                )
            })
            .disabled(!enabled)
            .on_click(cx.listener(move |app, _, window, cx| {
                app.refresh_desktop_content_tab(&id, window, cx)
            }))
    }

    pub(in crate::workspace::desktop::drawers::content) fn render_content_refresh_status(
        &self,
        id: &str,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        use super::refresh::ContentRefreshStatus;
        let theme = self.desktop_theme();
        let status = self.content_tab_refresh_status(id)?;
        let key = match status {
            ContentRefreshStatus::Loading if self.content_tab_is_refreshing(id) => {
                "drawers.refreshing_tab"
            }
            ContentRefreshStatus::Loading => return None,
            ContentRefreshStatus::Updated => "drawers.tab_refreshed",
            ContentRefreshStatus::Failed => "drawers.tab_refresh_failed",
        };
        let id = id.to_owned();
        Some(
            div()
                .id("desktop-content-refresh-status")
                .debug_selector(|| "desktop-content-refresh-status".into())
                .flex()
                .flex_none()
                .items_center()
                .gap(px(5.0))
                .text_size(px(11.0))
                .text_color(rgba(if status == ContentRefreshStatus::Failed {
                    theme.destructive
                } else {
                    theme.accent
                }))
                .child(ui::i18n::text(key))
                .when(status == ContentRefreshStatus::Failed, |status| {
                    status.child(
                        Button::new("desktop-content-refresh-retry")
                            .label(ui::i18n::text("drawers.retry_refresh_tab"))
                            .h(px(26.0))
                            .px(px(6.0))
                            .custom(self.reader_control_variant(reader_chrome_colors(theme), cx))
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.refresh_desktop_content_tab(&id, window, cx)
                            })),
                    )
                })
                .into_any_element(),
        )
    }
}
