use super::{with_alpha, SettingsPanelState, SettingsSection};
use crate::app::MultisoulApp;
use crate::settings::agents::{agents_settings, AgentDefinitionsState};
use crate::settings::voice::{voice_settings, VoiceSettingsState};
use crate::settings::wallpaper::{wallpaper_settings, WallpaperSettingsState};
use crate::shortcuts::ShortcutSettingsState;
use gpui::{div, prelude::*, px, rgba, Context, Div, FontWeight};
use ui::Theme;

mod hooks;
mod keyboard;

use self::hooks::hooks_settings;
use self::keyboard::keyboard_settings;

pub(super) fn settings_content(
    theme: Theme,
    panel: &SettingsPanelState,
    agent_definitions: &AgentDefinitionsState,
    voice_state: &VoiceSettingsState,
    wallpaper_state: &WallpaperSettingsState,
    shortcuts: &ShortcutSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .flex_1()
        .h_full()
        .flex()
        .flex_col()
        .bg(rgba(with_alpha(theme.term_bg, 0x72)))
        .child(content_header(theme, panel.active_section, cx))
        .child(
            div()
                .id("settings-content-scroll")
                .flex_1()
                .overflow_scroll()
                .px(px(26.0))
                .pb(px(28.0))
                .child(match panel.active_section {
                    SettingsSection::Keyboard => keyboard_settings(theme, shortcuts, cx),
                    SettingsSection::Voice => voice_settings(theme, voice_state, cx),
                    SettingsSection::Agents => agents_settings(theme, agent_definitions, cx),
                    SettingsSection::Hooks => hooks_settings(theme, panel, cx),
                    SettingsSection::Wallpaper => wallpaper_settings(theme, wallpaper_state, cx),
                    // Mock-only sections are hidden from the sidebar; keep a blank
                    // fallback so stale state or tests cannot route to old placeholders.
                    SettingsSection::General
                    | SettingsSection::Runtime
                    | SettingsSection::Skills => div(),
                }),
        )
}

fn content_header(theme: Theme, section: SettingsSection, cx: &mut Context<MultisoulApp>) -> Div {
    let subtitle = content_subtitle(section);
    let mut title_column = div().flex().flex_col().gap(px(3.0)).child(
        div()
            .text_size(px(21.0))
            .font_weight(FontWeight::BOLD)
            .text_color(rgba(theme.text_primary))
            .child(content_title(section)),
    );
    if !subtitle.is_empty() {
        title_column = title_column.child(
            div()
                .text_size(px(16.0))
                .text_color(rgba(theme.text_tertiary))
                .child(subtitle),
        );
    }

    div()
        .h(px(100.0))
        .flex()
        .flex_row()
        .items_start()
        .justify_between()
        .pt(px(34.0))
        .px(px(26.0))
        .child(title_column)
        .child(
            div()
                .id("settings-close")
                .w(px(34.0))
                .h(px(34.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.0))
                .text_size(px(25.0))
                .line_height(px(26.0))
                .text_color(rgba(theme.text_secondary))
                .cursor_pointer()
                .hover(move |style| style.bg(rgba(with_alpha(theme.destructive, 0x10))))
                .on_click(cx.listener(|app, _, _window, cx| {
                    app.close_settings_panel(cx);
                }))
                .child("x"),
        )
}

fn content_title(section: SettingsSection) -> &'static str {
    match section {
        SettingsSection::General => "Themes",
        SettingsSection::Keyboard => "Keyboard Shortcuts",
        SettingsSection::Runtime => "Runtime",
        SettingsSection::Voice => "Voice",
        SettingsSection::Agents => "Agents",
        SettingsSection::Skills => "Skills",
        SettingsSection::Hooks => "Agent Hooks",
        SettingsSection::Wallpaper => "Wallpaper",
    }
}

fn content_subtitle(section: SettingsSection) -> &'static str {
    match section {
        SettingsSection::General => "Choose your style or customize your theme",
        SettingsSection::Keyboard => "Customize app commands and review system input behavior",
        SettingsSection::Runtime => "Configure shell behavior and live sessions",
        SettingsSection::Voice => "Configure local speech recognition and command planning",
        SettingsSection::Agents => "Choose default agents, permissions, and profiles",
        SettingsSection::Skills => "Manage installed skills and project skill sources",
        SettingsSection::Hooks => "",
        SettingsSection::Wallpaper => "Choose a background",
    }
}
