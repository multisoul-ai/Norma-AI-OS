use crate::{
    app::{AppScreen, MultisoulApp},
    effects::dispatch_receipt::{COMMAND_DOCK_BOTTOM_PX, COMMAND_DOCK_HEIGHT_PX},
    shortcuts::ShortcutCommandId,
};
use canvas::{AgentProfile, CanvasRect};
use gpui::{div, prelude::*, px, rgba, Context, Div, Keystroke, MouseButton, Stateful, Window};
use runtime::command::DockCommand;
use ui::{
    component::{input::Input, kbd::Kbd},
    icons::{toolbar_icon, with_alpha, ToolbarIcon},
    ThemeId,
};

// Six themes is the longest list. 40px heading + 6 × 54px rows + 8px inset = 372px.
const HEADER: f32 = 40.0;
const ROW: f32 = 54.0;
const INSET: f32 = 8.0;
pub(crate) const DOCK_RADIUS: f32 = 18.0;

/// The suggestion panel sits on the Dock material, which stays dark in light themes.
pub(crate) fn command_panel_theme(theme: ui::Theme) -> ui::Theme {
    crate::app::top_bar_theme(theme)
}

impl MultisoulApp {
    pub(crate) fn sync_command_drawer(&mut self, window: &Window) {
        let Some(state) = &mut self.command_dock else {
            return;
        };
        let show = state.visible
            && !state.sidebar_origin
            && matches!(
                self.screen,
                AppScreen::Canvas | AppScreen::DesktopWorkspace | AppScreen::Launchpad
            )
            && !self.settings_panel.open
            && !self.feature_guide.is_visible();
        let target = if show {
            state.drawer.draft.clone_from(&state.draft);
            state.drawer.error.clone_from(&state.error);
            state.drawer.reply.clone_from(&state.reply);
            let rows = if state.error.is_some() {
                1
            } else {
                state.suggestions().len().max(1)
            };
            let bottom = COMMAND_DOCK_BOTTOM_PX + COMMAND_DOCK_HEIGHT_PX;
            (if state.reply.is_some() {
                220.0
            } else {
                HEADER + ROW * rows as f32 + INSET
            })
            .min((f32::from(window.viewport_size().height) - bottom - 60.0).max(0.0))
        } else {
            0.0
        };
        state.drawer.retarget(target);
    }

    pub(crate) fn dock_top_radius(&self) -> f32 {
        self.command_dock.as_ref().map_or(DOCK_RADIUS, |state| {
            if self.settings_panel.open || self.feature_guide.is_visible() {
                DOCK_RADIUS
            } else {
                state.drawer.dock_top_radius(DOCK_RADIUS)
            }
        })
    }

    pub(crate) fn command_panel_rect(&self, width: f32, height: f32) -> Option<CanvasRect> {
        let state = self.command_dock.as_ref()?;
        if state.sidebar_origin
            || state.drawer.opacity() <= 0.0
            || self.settings_panel.open
            || self.feature_guide.is_visible()
        {
            return None;
        }
        let panel_width = 840.0_f32.min((width - 36.0).max(0.0));
        let bottom = COMMAND_DOCK_BOTTOM_PX + COMMAND_DOCK_HEIGHT_PX;
        let panel_height = state
            .drawer
            .height(self.motion.reduce_motion)
            .min((height - bottom - 60.0).max(0.0));
        if panel_height <= 0.0 {
            return None;
        }
        Some(CanvasRect::new(
            (width - panel_width) / 2.0,
            height - bottom - panel_height,
            panel_width,
            panel_height,
        ))
    }

    pub(crate) fn render_command_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.ensure_command_input(window, cx);
        let state = self
            .command_dock
            .as_ref()
            .expect("command input initialized");
        let input = state.input.clone();
        let sidebar_origin = state.sidebar_origin;
        let focused = !sidebar_origin && self.command_input_focused(window, cx);
        let narrow_desktop = self
            .desktop_embedded_canvas_size
            .is_some_and(|size| size.x < 650.0)
            || matches!(self.screen, AppScreen::DesktopWorkspace)
                && self.desktop_assistant_layout(window).canvas_width < 650.0;
        let focus_shortcut = self
            .shortcuts
            .effective
            .iter()
            .find(|entry| entry.command == ShortcutCommandId::FocusCommandInput)
            .and_then(|entry| entry.current_keystroke.as_deref())
            .and_then(|key| Keystroke::parse(key).ok())
            .filter(|_| !focused);
        let theme = self.theme;
        div()
            .id("command-dock-input")
            .debug_selector(|| "command-dock-input".into())
            .w(px(360.0))
            .min_w(px(if narrow_desktop { 150.0 } else { 240.0 }))
            .flex_none()
            .flex_shrink()
            .h(px(34.0))
            .flex()
            .items_center()
            .px(px(8.0))
            .rounded(px(9.0))
            .border_1()
            .border_color(rgba(if focused {
                theme.accent_cyan
            } else {
                theme.dock_line
            }))
            .bg(rgba(with_alpha(theme.panel, 0xd8)))
            .on_click(cx.listener(|app, _, window, cx| app.focus_command_input(window, cx)))
            .child(
                Input::new(&input)
                    .appearance(false)
                    .cleanable(true)
                    .text_size(px(13.0))
                    .when_some(focus_shortcut, |input, key| {
                        input.suffix(
                            Kbd::new(key)
                                .text_size(px(11.0))
                                .text_color(rgba(theme.text_secondary))
                                .border_color(rgba(theme.dock_line))
                                .bg(rgba(0x00000000)),
                        )
                    }),
            )
    }

    pub(crate) fn render_command_suggestions(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let viewport = window.viewport_size();
        let rect =
            self.command_panel_rect(f32::from(viewport.width), f32::from(viewport.height))?;
        let radius = px(DOCK_RADIUS);
        let state = self.command_dock.as_ref()?;
        let items = super::CommandDockState::suggestions_for(&state.drawer.draft);
        let theme = command_panel_theme(self.theme);
        let is_conversation = items.is_empty() && !state.drawer.draft.trim().starts_with('/');
        let heading = if is_conversation {
            "Norma"
        } else if state.drawer.draft.trim().starts_with("/new") {
            ui::i18n::text("chrome.new_agent")
        } else if items
            .iter()
            .all(|s| matches!(s.command, Some(DockCommand::Theme(_))))
            && !items.is_empty()
        {
            ui::i18n::text("chrome.apply_theme")
        } else {
            ui::i18n::text("chrome.commands")
        };
        let mut rows = div()
            .id("command-dock-results")
            .overflow_y_scroll()
            .track_scroll(&state.scroll)
            .min_h(px(0.0))
            .flex_1();
        if let Some(error) = &state.drawer.error {
            rows = rows.child(
                div()
                    .p(px(12.0))
                    .text_color(rgba(theme.destructive))
                    .child(error.clone()),
            );
        } else if let Some(reply) = &state.drawer.reply {
            rows = rows.child(
                div()
                    .id("norma-dock-reply")
                    .debug_selector(|| "norma-dock-reply".into())
                    .p(px(12.0))
                    .text_size(px(13.0))
                    .whitespace_normal()
                    .child(reply.clone()),
            );
        } else if is_conversation {
            rows = rows.child(
                div()
                    .id("norma-dock-send")
                    .p(px(12.0))
                    .rounded(px(10.0))
                    .cursor_pointer()
                    .hover(move |s| s.bg(rgba(with_alpha(theme.accent_blue, 0x38))))
                    .active(move |s| s.bg(rgba(with_alpha(theme.accent_blue, 0x55))))
                    .on_click(
                        cx.listener(|app, _, window, cx| app.submit_dock_conversation(window, cx)),
                    )
                    .child(ui::i18n::text("chrome.message_norma_press_return_to_send")),
            );
        } else if items.is_empty() {
            rows = rows.child(
                div()
                    .p(px(12.0))
                    .text_color(rgba(theme.text_secondary))
                    .child(ui::i18n::text(
                        "chrome.no_commands_found_type_to_see_available_actions",
                    )),
            );
        } else {
            for (index, item) in items.iter().enumerate() {
                let selected = index == state.selected;
                let icon = match &item.command {
                    Some(DockCommand::Agent(p)) => match p {
                        AgentProfile::Codex => ToolbarIcon::Codex,
                        AgentProfile::Claude => ToolbarIcon::Claude,
                        AgentProfile::OpenCode => ToolbarIcon::OpenCode,
                        AgentProfile::Cursor => ToolbarIcon::Cursor,
                        AgentProfile::Grok => ToolbarIcon::Grok,
                        AgentProfile::Kimi => ToolbarIcon::Kimi,
                        AgentProfile::ZCode => ToolbarIcon::ZCode,
                        AgentProfile::Pi => ToolbarIcon::Pi,
                        AgentProfile::Hermes => ToolbarIcon::Hermes,
                    },
                    Some(DockCommand::Terminal) => ToolbarIcon::Terminal,
                    Some(DockCommand::Browser) => ToolbarIcon::Browser,
                    Some(DockCommand::Preview) => ToolbarIcon::Preview,
                    Some(DockCommand::Theme(_)) => ToolbarIcon::Settings,
                    None if item.completion.starts_with("/theme") => ToolbarIcon::Settings,
                    None => ToolbarIcon::Plus,
                };
                let detail = match &item.command {
                    Some(DockCommand::Theme(id)) => ThemeId::ALL
                        .into_iter()
                        .find(|theme| theme.as_str() == id)
                        .map(ThemeId::description)
                        .unwrap_or(""),
                    Some(DockCommand::Preview) => ui::i18n::text("chrome.choose_a_file_to_preview"),
                    Some(_) => ui::i18n::text("chrome.create_a_card_in_this_workspace"),
                    None => ui::i18n::text("chrome.choose_an_action"),
                };
                rows = rows.child(
                    div()
                        .id(("command-option", index))
                        .h(px(ROW))
                        .flex_none()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .px(px(12.0))
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(rgba(if selected {
                            with_alpha(theme.accent_cyan, 0x80)
                        } else {
                            0x00000000
                        }))
                        .bg(rgba(if selected {
                            with_alpha(theme.accent_blue, 0x2e)
                        } else {
                            0x00000000
                        }))
                        .cursor_pointer()
                        .hover(move |s| s.bg(rgba(with_alpha(theme.accent_blue, 0x38))))
                        .active(move |s| s.bg(rgba(with_alpha(theme.accent_blue, 0x55))))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |app, _, window, cx| {
                            if let Some(state) = &mut app.command_dock {
                                state.selected = index;
                            }
                            app.execute_dock_suggestion(window, cx);
                        }))
                        .child(toolbar_icon(icon, theme.text_primary, 20.0))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .child(div().text_size(px(13.0)).child(suggestion_title(item)))
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(rgba(theme.text_secondary))
                                        .child(detail),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgba(theme.text_secondary))
                                .child(item.completion.trim().to_string()),
                        ),
                );
            }
        }
        let content = div()
            .h(px((state.drawer.content_height - INSET * 2.0).max(0.0)))
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(HEADER - INSET))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(12.0))
                    .text_size(px(11.0))
                    .child(heading)
                    .child(if is_conversation {
                        ui::i18n::text("chrome.chat").into()
                    } else {
                        ui::i18n::format(
                            "chrome.suggestions",
                            &[("arg0", (items.len()).to_string())],
                        )
                    }),
            )
            .child(rows);
        Some(
            div()
                .id("command-dock-suggestions")
                .debug_selector(|| "command-dock-suggestions".into())
                // Anchored to the dock's top edge; -1px insets cover the dock border so both
                // surfaces share one outer edge.
                .absolute()
                .left(px(-1.0))
                .right(px(-1.0))
                // Padding box is dock height - 2×1px border; 51px lifts the panel's bottom
                // to the dock's outer top edge, leaving its top border as the divider.
                .bottom(px(COMMAND_DOCK_HEIGHT_PX - 1.0))
                .h(px(rect.size.y))
                .overflow_hidden()
                .opacity(state.drawer.opacity())
                .rounded_tl(radius)
                .rounded_tr(radius)
                .border_1()
                .border_b_0()
                .border_color(rgba(theme.dock_line))
                .bg(rgba(theme.dock))
                .text_color(rgba(theme.text_primary))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down_out(cx.listener(|app, _, _, cx| {
                    app.hide_command_suggestions();
                    cx.notify();
                }))
                .child(
                    // GPUI clips rectangularly: inset content on every side to keep row
                    // backgrounds out of the shell's rounded corners during the reveal.
                    div()
                        .absolute()
                        .top(px(INSET))
                        .left(px(INSET))
                        .right(px(INSET))
                        .h(px((rect.size.y - INSET * 2.0).max(0.0)))
                        .overflow_hidden()
                        .child(content),
                ),
        )
    }
}

fn suggestion_title(item: &runtime::command::CommandSuggestion) -> String {
    match &item.command {
        Some(DockCommand::Agent(profile)) => ui::i18n::format(
            "chrome.command.new_agent",
            &[("profile", profile.display_title().to_string())],
        ),
        Some(DockCommand::Terminal) => ui::i18n::text("chrome.command.terminal").into(),
        Some(DockCommand::Browser) => ui::i18n::text("chrome.command.browser").into(),
        Some(DockCommand::Preview) => ui::i18n::text("chrome.command.preview").into(),
        Some(DockCommand::Theme(_)) => item.title.clone(),
        None if item.completion.starts_with("/new") => ui::i18n::text("chrome.new_agent").into(),
        None => ui::i18n::text("chrome.command.theme").into(),
    }
}
