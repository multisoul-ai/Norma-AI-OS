use super::glass_button_variant;
use crate::app::{AppScreen, MultisoulApp};
use gpui::{div, prelude::*, px, rgba, Context, Div, Focusable, Stateful, Window};
use ui::component::button::{Button, ButtonVariants};
use ui::component::input::{Input, InputEvent, InputState};
use ui::component::{Disableable, StyledExt};

pub(super) fn desktop_form_header(title: &'static str, subtitle: String, theme: ui::Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(14.0))
        .child(
            div()
                .flex_none()
                .w(px(44.0))
                .h(px(44.0))
                .rounded(px(12.0))
                .bg(rgba(ui::icons::with_alpha(theme.accent_blue, 0x18)))
                .flex()
                .items_center()
                .justify_center()
                .child(ui::icons::toolbar_icon(
                    ui::icons::ToolbarIcon::Workspaces,
                    theme.accent_blue,
                    24.0,
                )),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(5.0))
                .child(
                    div()
                        .text_size(px(20.0))
                        .font_semibold()
                        .text_color(rgba(theme.text_primary))
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgba(theme.text_secondary))
                        .text_ellipsis()
                        .child(subtitle),
                ),
        )
}

impl MultisoulApp {
    pub(crate) fn open_desktop_switcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            // Returning can detach this project; release its current borrow first.
            window.defer(cx, move |window, cx| {
                host.update(cx, |host, cx| host.open_desktop_switcher(window, cx));
            });
            return;
        }
        if self.desktop_workspace.switcher_open {
            self.dismiss_desktop_switcher(window, cx);
            return;
        }
        let return_to = matches!(self.screen, AppScreen::DesktopWorkspace)
            .then(|| self.desktop_workspace.active_id.clone())
            .flatten()
            .map(|id| (id, self.desktop_workspace.expanded_area_id.clone()));
        self.show_desktop_directory(false, window, cx);
        let input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(ui::i18n::input_placeholder(
                "workspace.search_workspaces",
                window,
                cx,
            ))
        });
        let subscription = cx.subscribe(&input, |app, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let query = input.read(cx).value().to_string();
                app.desktop_workspace.switcher_query = query.clone();
                app.desktop_workspace.search_query = query;
                cx.notify();
            }
        });
        input.focus_handle(cx).focus(window);
        self.desktop_workspace.switcher_input = Some(input);
        self.desktop_switcher_subscription = Some(subscription);
        self.desktop_workspace.search_query.clear();
        self.desktop_workspace.switcher_query.clear();
        self.desktop_workspace.switcher_return = return_to;
        self.desktop_workspace.switcher_open = true;
        cx.notify();
    }

    pub(crate) fn dismiss_desktop_switcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let return_to = self.desktop_workspace.switcher_return.take();
        self.close_desktop_switcher(cx);
        self.desktop_workspace.pending_navigation = None;
        window.focus(&self.focus_handle);
        if let Some((workspace_id, area_id)) = return_to {
            if let Some(area_id) = area_id {
                self.expand_desktop_project_area(area_id, window, cx);
            } else {
                self.begin_desktop_workspace_zoom(&workspace_id, false, window, cx);
            }
            self.screen = AppScreen::DesktopWorkspace;
        }
        cx.notify();
    }

    pub(crate) fn close_desktop_switcher(&mut self, cx: &mut Context<Self>) {
        self.desktop_workspace.switcher_open = false;
        self.desktop_workspace.switcher_return = None;
        self.desktop_workspace.switcher_input = None;
        self.desktop_switcher_subscription = None;
        self.desktop_workspace.switcher_query.clear();
        cx.notify();
    }

    // Only controls live here. The directory and its camera are rendered once by launchpad.
    pub(crate) fn render_desktop_switcher(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let input = self.desktop_workspace.switcher_input.clone();
        div()
            .id("desktop-workspace-switcher")
            .debug_selector(|| "desktop-workspace-switcher".to_string())
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .flex()
            .items_center()
            .h(px(62.0))
            .gap(px(20.0))
            .px(px(56.0))
            .py(px(12.0))
            .child(div().flex_1())
            .when_some(input, |this, input| {
                this.child(
                    div()
                        .w(px(280.0))
                        .h(px(38.0))
                        .flex()
                        .items_center()
                        .px(px(10.0))
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(rgba(theme.dock_line))
                        .bg(rgba(self.desktop_input_fill()))
                        .text_color(rgba(theme.text_primary))
                        .child(Input::new(&input).appearance(false).cleanable(true)),
                )
            })
            .child(
                Button::new("desktop-workspace-switch-create")
                    .debug_selector(|| "desktop-workspace-switch-create".into())
                    .label(ui::i18n::text("shortcuts.new_workspace"))
                    .custom(glass_button_variant(theme, cx))
                    .disabled(self.desktop_workspace.busy)
                    .on_click(
                        cx.listener(|app, _, window, cx| app.create_desktop_workspace(window, cx)),
                    ),
            )
    }
}
