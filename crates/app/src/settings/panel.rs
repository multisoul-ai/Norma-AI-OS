use crate::app::{AppScreen, MultisoulApp, ToggleSettings};
use gpui::{
    div, point, prelude::*, px, rgba, BoxShadow, ClickEvent, Context, Div, FontWeight, MouseButton,
    Window,
};
use ui::Theme;

#[path = "content.rs"]
mod content;

use self::content::settings_content;

const PANEL_MIN_W: f32 = 860.0;
const PANEL_MAX_W: f32 = 1180.0;
const PANEL_MIN_H: f32 = 620.0;
const PANEL_MAX_H: f32 = 760.0;
const SIDEBAR_W: f32 = 330.0;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum SettingsSection {
    General,
    Keyboard,
    Runtime,
    Voice,
    Agents,
    Skills,
    Hooks,
    Wallpaper,
}

const SETTINGS_SECTION_ORDER: [SettingsSection; 8] = [
    SettingsSection::General,
    SettingsSection::Keyboard,
    SettingsSection::Runtime,
    SettingsSection::Voice,
    SettingsSection::Agents,
    SettingsSection::Skills,
    SettingsSection::Hooks,
    SettingsSection::Wallpaper,
];

impl SettingsSection {
    fn title(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Keyboard => "Keyboard Shortcuts",
            Self::Runtime => "Runtime",
            Self::Voice => "Voice",
            Self::Agents => "Agents",
            Self::Skills => "Skills",
            Self::Hooks => "Hooks",
            Self::Wallpaper => "Wallpaper",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::General => "⚙",
            Self::Keyboard => "⌘",
            Self::Runtime => ">_",
            Self::Voice => "V",
            Self::Agents => "A",
            Self::Skills => "✦",
            Self::Hooks => "⛓",
            // 🖼 emoji is risky in the bundled icon font; reuse the single-ASCII
            // letter convention used by Voice/Agents for consistency.
            Self::Wallpaper => "W",
        }
    }

    fn index(self) -> usize {
        for (index, section) in SETTINGS_SECTION_ORDER.iter().enumerate() {
            if *section == self {
                return index;
            }
        }
        0
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct HookInstallStatus {
    pub(crate) message: Option<String>,
    pub(crate) is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SettingsPanelState {
    pub(crate) open: bool,
    pub(crate) active_section: SettingsSection,
    pub(crate) codex_hook_status: HookInstallStatus,
    pub(crate) claude_hook_status: HookInstallStatus,
}

impl Default for SettingsPanelState {
    fn default() -> Self {
        Self {
            open: false,
            active_section: SettingsSection::General,
            codex_hook_status: HookInstallStatus::default(),
            claude_hook_status: HookInstallStatus::default(),
        }
    }
}

impl SettingsPanelState {
    pub(crate) fn open(&mut self) {
        self.open = true;
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
    }

    pub(crate) fn toggle(&mut self) {
        if self.open {
            self.close();
        } else {
            self.open();
        }
    }

    pub(crate) fn select(&mut self, section: SettingsSection) {
        self.active_section = section;
    }
}

impl MultisoulApp {
    pub(crate) fn on_toolbar_settings(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_settings_panel(cx);
    }

    pub(crate) fn on_toggle_settings(
        &mut self,
        _: &ToggleSettings,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_settings_panel(cx);
    }

    pub(crate) fn toggle_settings_panel(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.screen, AppScreen::Canvas) {
            return;
        }
        let was_open = self.settings_panel.open;
        self.settings_panel.toggle();
        if was_open {
            self.shortcuts.recording = None;
        }
        cx.notify();
    }

    pub(crate) fn close_settings_panel(&mut self, cx: &mut Context<Self>) {
        self.settings_panel.close();
        self.shortcuts.recording = None;
        cx.notify();
    }

    pub(crate) fn render_settings_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let win = window.viewport_size();
        let win_w = f32::from(win.width);
        let win_h = f32::from(win.height);
        let available_w = (win_w - 64.0).max(360.0);
        let available_h = (win_h - 64.0).max(360.0);
        let panel_w = available_w
            .min(PANEL_MAX_W)
            .max(PANEL_MIN_W.min(available_w));
        let panel_h = available_h
            .min(PANEL_MAX_H)
            .max(PANEL_MIN_H.min(available_h));
        let left = ((win_w - panel_w) * 0.5).max(24.0);
        let top = ((win_h - panel_h) * 0.5).max(38.0);

        div()
            .id("settings-overlay")
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .occlude()
            .bg(rgba(with_alpha(theme.graphite, 0x46)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("settings-panel")
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .w(px(panel_w))
                    .h(px(panel_h))
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    .rounded(px(18.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.edge_highlight, 0x46)))
                    .shadow(settings_shadow(theme))
                    .bg(rgba(with_alpha(theme.panel, 0xf4)))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(self.render_settings_sidebar(theme, cx))
                    .child(settings_content(
                        theme,
                        &self.settings_panel,
                        &self.agent_definitions,
                        &self.voice_settings,
                        &self.wallpaper_settings,
                        &self.shortcuts,
                        cx,
                    )),
            )
    }

    fn render_settings_sidebar(&mut self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let active = self.settings_panel.active_section;

        div()
            .w(px(SIDEBAR_W))
            .h_full()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .px(px(22.0))
            .py(px(26.0))
            .border_r_1()
            .border_color(rgba(with_alpha(theme.edge_highlight, 0x24)))
            .bg(rgba(with_alpha(theme.node_fill, 0xf2)))
            .child(
                div()
                    .mb(px(16.0))
                    .text_size(px(28.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgba(theme.text_primary))
                    .child("Settings"),
            )
            // [MOCK] General/Themes 是占位实现（appearance.rs 无 on_click、硬编码选中），暂时屏蔽。
            // .child(section_button(theme, SettingsSection::General, active, cx))
            // .child(sidebar_child(theme, "Language & Region", false))
            // .child(sidebar_child(
            //     theme,
            //     "Appearance",
            //     active == SettingsSection::General,
            // ))
            // .child(sidebar_child(theme, "Date & Time", false))
            .child(section_button(theme, SettingsSection::Keyboard, active, cx))
            .child(sidebar_gap())
            // [MOCK] Runtime/Agents/Skills 走 mock_section + 硬编码假数据，暂时屏蔽。
            // .child(section_button(theme, SettingsSection::Runtime, active, cx))
            .child(section_button(theme, SettingsSection::Voice, active, cx))
            .child(section_button(theme, SettingsSection::Agents, active, cx))
            // [MOCK] .child(section_button(theme, SettingsSection::Skills, active, cx))
            .child(section_button(theme, SettingsSection::Hooks, active, cx))
            .child(section_button(
                theme,
                SettingsSection::Wallpaper,
                active,
                cx,
            ))
    }
}

fn section_button(
    theme: Theme,
    section: SettingsSection,
    active: SettingsSection,
    cx: &mut Context<MultisoulApp>,
) -> impl IntoElement {
    let is_active = section == active;
    let (bg, text, chevron) = if is_active {
        (
            with_alpha(theme.accent_orange, 0x18),
            theme.accent_orange,
            if section == SettingsSection::General {
                "^"
            } else {
                ">"
            },
        )
    } else {
        (0x00000000, theme.text_secondary, ">")
    };

    div()
        .id(("settings-section", section.index()))
        .h(px(42.0))
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px(px(18.0))
        .rounded(px(7.0))
        .bg(rgba(bg))
        .text_color(rgba(text))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(with_alpha(theme.panel_high, 0xa6))))
        .on_click(cx.listener(move |app, _, _window, cx| {
            app.settings_panel.select(section);
            cx.notify();
        }))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(12.0))
                .child(section_icon(theme, section, is_active))
                .child(
                    div()
                        .text_size(px(15.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(section.title()),
                ),
        )
        .child(
            div()
                .text_size(px(18.0))
                .text_color(rgba(theme.text_tertiary))
                .child(chevron),
        )
}

fn section_icon(theme: Theme, section: SettingsSection, active: bool) -> Div {
    div()
        .w(px(22.0))
        .h(px(22.0))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(if section == SettingsSection::Runtime {
            11.0
        } else {
            18.0
        }))
        .font_weight(FontWeight::BOLD)
        .text_color(rgba(if active {
            theme.accent_orange
        } else {
            theme.text_secondary
        }))
        .child(section.icon())
}

fn sidebar_gap() -> Div {
    div().h(px(8.0))
}

fn settings_shadow(theme: Theme) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: rgba(with_alpha(theme.shadow_color, 0x9a)).into(),
        offset: point(px(0.0), px(26.0)),
        blur_radius: px(58.0),
        spread_radius: px(-22.0),
    }]
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}

#[cfg(test)]
#[path = "../../tests/unit/settings_panel_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/unit/settings_panel_voice_tests.rs"]
mod voice_tests;
