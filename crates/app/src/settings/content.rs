use super::{with_alpha, SettingsPanelState, SettingsSection};
use crate::app::MultisoulApp;
use crate::settings::agents::{agents_settings, AgentDefinitionsState};
use crate::settings::appearance::theme_appearance;
use crate::settings::voice::{
    voice_settings, VoiceSettingsInputs, VoiceSettingsState, VoiceSetupDestination,
};
use crate::settings::wallpaper::{wallpaper_settings, WallpaperSettingsState};
use crate::shortcuts::ShortcutSettingsState;
use gpui::{div, prelude::*, px, rgba, Context, Div, FontWeight, ScrollHandle, Window};
use ui::{Theme, ThemeId};

mod hooks;
mod keyboard;

use self::hooks::hooks_settings;
use self::keyboard::keyboard_settings;

pub(super) struct VoiceSettingsContent<'a> {
    pub(super) state: &'a VoiceSettingsState,
    pub(super) inputs: &'a VoiceSettingsInputs,
    pub(super) setup_scroll_target: Option<VoiceSetupDestination>,
}

pub(super) fn settings_content(
    theme: Theme,
    theme_id: ThemeId,
    panel: &SettingsPanelState,
    agent_definitions: &AgentDefinitionsState,
    voice: VoiceSettingsContent<'_>,
    content_scroll: &ScrollHandle,
    wallpaper_state: &WallpaperSettingsState,
    shortcuts: &ShortcutSettingsState,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .h_full()
        .flex()
        .flex_col()
        .bg(rgba(with_alpha(theme.panel_high, 0xd4)))
        .child(content_header(theme, panel.active_section, cx))
        .child(
            div()
                .id("settings-content-scroll")
                .debug_selector(|| "settings-content-scroll".to_string())
                .flex_1()
                .min_w(px(0.0))
                .min_h(px(0.0))
                .overflow_y_scroll()
                .overflow_x_hidden()
                .track_scroll(content_scroll)
                .px(px(28.0))
                .pb(px(40.0))
                .child(match panel.active_section {
                    SettingsSection::Themes => theme_appearance(theme_id, theme, cx),
                    SettingsSection::Keyboard => keyboard_settings(theme, shortcuts, cx),
                    SettingsSection::Voice => voice_settings(
                        theme,
                        voice.state,
                        voice.inputs,
                        content_scroll,
                        voice.setup_scroll_target,
                        cx,
                    ),
                    SettingsSection::Agents => {
                        agents_settings(theme, agent_definitions, window, cx)
                    }
                    SettingsSection::Hooks => hooks_settings(theme, panel, cx),
                    SettingsSection::Wallpaper => wallpaper_settings(theme, wallpaper_state, cx),
                    // Mock-only sections are hidden from the sidebar; keep a blank
                    // fallback so stale state or tests cannot route to old placeholders.
                    #[cfg(test)]
                    SettingsSection::Runtime | SettingsSection::Skills => div(),
                }),
        )
}

fn content_header(theme: Theme, section: SettingsSection, cx: &mut Context<MultisoulApp>) -> Div {
    let subtitle = content_subtitle(section);
    let mut title_column = div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(3.0))
        .child(
            div()
                .text_size(px(24.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(theme.text_primary))
                .child(content_title(section)),
        );
    if !subtitle.is_empty() {
        title_column = title_column.child(
            div()
                .text_size(px(14.0))
                .text_color(rgba(theme.text_tertiary))
                .child(subtitle),
        );
    }

    div()
        .min_h(px(104.0))
        .flex()
        .flex_row()
        .items_start()
        .justify_between()
        .pt(px(30.0))
        .px(px(28.0))
        .pb(px(18.0))
        .border_b_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x24)))
        .child(title_column)
        .child(
            div()
                .id("settings-close")
                .debug_selector(|| "settings-close".to_string())
                .w(px(34.0))
                .h(px(34.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(17.0))
                .text_size(px(22.0))
                .line_height(px(26.0))
                .text_color(rgba(theme.text_secondary))
                .cursor_pointer()
                .hover(move |style| style.bg(rgba(with_alpha(theme.destructive, 0x10))))
                .on_click(cx.listener(|app, _, _window, cx| {
                    app.close_settings_panel(cx);
                }))
                .child("×"),
        )
}

fn content_title(section: SettingsSection) -> &'static str {
    match section {
        SettingsSection::Themes => "Themes",
        SettingsSection::Keyboard => "Keyboard Shortcuts",
        #[cfg(test)]
        SettingsSection::Runtime => "Runtime",
        SettingsSection::Voice => "Voice",
        SettingsSection::Agents => "Agents",
        #[cfg(test)]
        SettingsSection::Skills => "Skills",
        SettingsSection::Hooks => "Agent Hooks",
        SettingsSection::Wallpaper => "Wallpaper",
    }
}

fn content_subtitle(section: SettingsSection) -> &'static str {
    match section {
        SettingsSection::Themes => "Choose one of the six reviewed Core UI V2 palettes",
        SettingsSection::Keyboard => "Customize app commands and review system input behavior",
        #[cfg(test)]
        SettingsSection::Runtime => "Configure shell behavior and live sessions",
        SettingsSection::Voice => "Use ⌘E to talk to Norma",
        SettingsSection::Agents => "Choose default agents, permissions, and profiles",
        #[cfg(test)]
        SettingsSection::Skills => "Manage installed skills and project skill sources",
        SettingsSection::Hooks => "",
        SettingsSection::Wallpaper => "Choose a background",
    }
}
