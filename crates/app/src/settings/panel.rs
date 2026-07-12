use crate::app::MultisoulApp;
use gpui::{
    div, prelude::*, px, rgba, Context, Div, FontWeight, KeyDownEvent, MouseButton, Window,
};
use ui::Theme;

#[path = "content.rs"]
mod content;
#[path = "panel_style.rs"]
mod panel_style;

use self::content::settings_content;
use self::panel_style::{settings_shadow, with_alpha};

const PANEL_MIN_W: f32 = 860.0;
const PANEL_MAX_W: f32 = 1180.0;
const PANEL_MIN_H: f32 = 620.0;
const PANEL_MAX_H: f32 = 760.0;
const SIDEBAR_MIN_W: f32 = 160.0;
const SIDEBAR_MAX_W: f32 = 280.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct SettingsPanelLayout {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    sidebar_width: f32,
}

impl SettingsPanelLayout {
    fn for_viewport(width: f32, height: f32) -> Self {
        let margin = if width < 640.0 || height < 520.0 {
            12.0
        } else {
            32.0
        };
        let available_width = (width - margin * 2.0).max(1.0);
        let available_height = (height - margin * 2.0).max(1.0);
        let panel_width = available_width
            .min(PANEL_MAX_W)
            .max(PANEL_MIN_W.min(available_width));
        let panel_height = available_height
            .min(PANEL_MAX_H)
            .max(PANEL_MIN_H.min(available_height));
        let sidebar_width = (panel_width * 0.28)
            .clamp(SIDEBAR_MIN_W, SIDEBAR_MAX_W)
            .min((panel_width * 0.45).max(1.0));

        Self {
            left: ((width - panel_width) * 0.5).max(0.0),
            top: ((height - panel_height) * 0.5).max(0.0),
            width: panel_width,
            height: panel_height,
            sidebar_width,
        }
    }
}

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
    pub(crate) fn title(self) -> &'static str {
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
    pub(crate) opencode_hook_status: HookInstallStatus,
}

impl Default for SettingsPanelState {
    fn default() -> Self {
        Self {
            open: false,
            active_section: SettingsSection::General,
            codex_hook_status: HookInstallStatus::default(),
            claude_hook_status: HookInstallStatus::default(),
            opencode_hook_status: HookInstallStatus::default(),
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
    /// Settings 面板 KeyDown 入口。
    pub(crate) fn on_settings_panel_key_down(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.handle_settings_text_key_down(ev, cx) {
            return;
        }
        if self.voice_settings_inputs.is_focused(window, cx) {
            return;
        }
        if self.shortcuts.recording.is_some() {
            cx.stop_propagation();
        }
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
        let layout = SettingsPanelLayout::for_viewport(win_w, win_h);

        // overlay 挡住自动聚焦；点击时显式 focus。
        div()
            .id("settings-overlay")
            .debug_selector(|| "settings-overlay".to_string())
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .occlude()
            .bg(rgba(with_alpha(theme.graphite, 0x46)))
            .on_mouse_down(MouseButton::Left, {
                let focus = self.focus_handle.clone();
                move |_, window, cx| {
                    focus.focus(window);
                    cx.stop_propagation();
                }
            })
            .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("settings-panel")
                    .debug_selector(|| "settings-panel".to_string())
                    .track_focus(&self.focus_handle)
                    .absolute()
                    .left(px(layout.left))
                    .top(px(layout.top))
                    .w(px(layout.width))
                    .h(px(layout.height))
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    .rounded(px(18.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.edge_highlight, 0x46)))
                    .shadow(settings_shadow(theme))
                    .bg(rgba(with_alpha(theme.panel, 0xf4)))
                    .on_mouse_down(MouseButton::Left, {
                        let focus = self.focus_handle.clone();
                        move |_, window, cx| {
                            focus.focus(window);
                            cx.stop_propagation();
                        }
                    })
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .on_key_down(cx.listener(Self::on_settings_panel_key_down))
                    .child(self.render_settings_sidebar(theme, layout.sidebar_width, cx))
                    .child(settings_content(
                        theme,
                        &self.settings_panel,
                        &self.agent_definitions,
                        &self.voice_settings,
                        &self.voice_settings_inputs,
                        &self.wallpaper_settings,
                        &self.shortcuts,
                        cx,
                    )),
            )
    }

    fn render_settings_sidebar(
        &mut self,
        theme: Theme,
        sidebar_w: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.settings_panel.active_section;
        let compact = sidebar_w < 220.0;

        div()
            .id("settings-sidebar-scroll")
            .debug_selector(|| "settings-sidebar-scroll".to_string())
            .w(px(sidebar_w))
            .min_w(px(sidebar_w))
            .h_full()
            .overflow_y_scroll()
            .overflow_x_hidden()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .px(px(if compact { 12.0 } else { 22.0 }))
            .py(px(if compact { 18.0 } else { 26.0 }))
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
            //     theme,
            //     "Appearance",
            //     active == SettingsSection::General,
            // ))
            .child(section_button(
                theme,
                SettingsSection::Keyboard,
                active,
                compact,
                cx,
            ))
            .child(sidebar_gap())
            .child(section_button(
                theme,
                SettingsSection::Voice,
                active,
                compact,
                cx,
            ))
            .child(section_button(
                theme,
                SettingsSection::Agents,
                active,
                compact,
                cx,
            ))
            .child(section_button(
                theme,
                SettingsSection::Hooks,
                active,
                compact,
                cx,
            ))
            .child(section_button(
                theme,
                SettingsSection::Wallpaper,
                active,
                compact,
                cx,
            ))
    }
}

fn section_button(
    theme: Theme,
    section: SettingsSection,
    active: SettingsSection,
    compact: bool,
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
        .debug_selector(move || settings_section_debug_selector(section).to_string())
        .h(px(42.0))
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px(px(if compact { 10.0 } else { 18.0 }))
        .rounded(px(7.0))
        .bg(rgba(bg))
        .text_color(rgba(text))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(with_alpha(theme.panel_high, 0xa6))))
        .on_click(cx.listener(move |app, _, _window, cx| {
            app.settings_panel.select(section);
            if section == SettingsSection::Voice {
                app.refresh_asr_model_states(cx);
            }
            cx.notify();
        }))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(if compact { 8.0 } else { 12.0 }))
                .child(section_icon(theme, section, is_active))
                .child(
                    div()
                        .min_w(px(0.0))
                        .overflow_hidden()
                        .truncate()
                        .text_size(px(15.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(section.title()),
                ),
        )
        .child(
            div()
                .flex_none()
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

fn settings_section_debug_selector(section: SettingsSection) -> &'static str {
    match section {
        SettingsSection::General => "settings-section-general",
        SettingsSection::Keyboard => "settings-section-keyboard",
        SettingsSection::Runtime => "settings-section-runtime",
        SettingsSection::Voice => "settings-section-voice",
        SettingsSection::Agents => "settings-section-agents",
        SettingsSection::Skills => "settings-section-skills",
        SettingsSection::Hooks => "settings-section-hooks",
        SettingsSection::Wallpaper => "settings-section-wallpaper",
    }
}

#[cfg(test)]
#[path = "../../tests/unit/settings_panel_tests.rs"]
mod settings_panel_tests;

#[cfg(test)]
#[path = "../../tests/unit/settings_panel_voice_tests.rs"]
mod settings_panel_tests_voice;
