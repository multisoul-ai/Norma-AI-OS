use super::{with_alpha, SettingsPanelState, SettingsSection};
use crate::app::MultisoulApp;
use crate::settings::agents::{agents_settings, AgentDefinitionsState};
use crate::settings::appearance::theme_appearance;
use crate::settings::orchestration::{orchestration_settings, OrchestrationSettingsState};
use crate::settings::performance::{performance_settings, PerformanceSettingsState};
use crate::settings::voice::{
    voice_settings, VoiceSettingsInputs, VoiceSettingsState, VoiceSetupDestination,
};
use crate::settings::wallpaper::{wallpaper_settings, PreviewCache, WallpaperSettingsState};
use crate::shortcuts::ShortcutSettingsState;
use gpui::{div, prelude::*, px, rgba, Context, Div, FontWeight, ScrollHandle, Window};
use ui::{Theme, ThemeId};

mod keyboard;

use self::keyboard::keyboard_settings;

pub(super) struct VoiceSettingsContent<'a> {
    pub(super) state: &'a VoiceSettingsState,
    pub(super) inputs: &'a VoiceSettingsInputs,
    pub(super) setup_scroll_target: Option<VoiceSetupDestination>,
}

pub(super) struct SettingsContentContext<'a> {
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) theme: Theme,
    pub(super) theme_id: ThemeId,
    pub(super) panel: &'a SettingsPanelState,
    pub(super) agent_definitions: &'a AgentDefinitionsState,
    pub(super) selected_agent_on_canvas: bool,
    pub(super) agent_on_canvas_ids: &'a std::collections::HashSet<i64>,
    pub(super) orchestration: &'a OrchestrationSettingsState,
    pub(super) performance: &'a PerformanceSettingsState,
    pub(super) voice: VoiceSettingsContent<'a>,
    pub(super) content_scroll: &'a ScrollHandle,
    pub(super) agent_list_scroll: &'a ScrollHandle,
    pub(super) wallpaper_state: &'a WallpaperSettingsState,
    pub(super) wallpaper_previews: &'a mut PreviewCache,
    pub(super) shortcuts: &'a ShortcutSettingsState,
    pub(super) mobile: gpui::Entity<crate::mobile::controller::MobileController>,
}

pub(super) fn settings_content(
    content: SettingsContentContext<'_>,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let SettingsContentContext {
        width,
        height,
        theme,
        theme_id,
        panel,
        agent_definitions,
        selected_agent_on_canvas,
        agent_on_canvas_ids,
        orchestration,
        performance,
        voice,
        content_scroll,
        agent_list_scroll,
        wallpaper_state,
        wallpaper_previews,
        shortcuts,
        mobile,
    } = content;
    let compact = width < 600.0 || height < 640.0;
    let padding = if compact { 16.0 } else { 28.0 };
    div()
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .h_full()
        .flex()
        .flex_col()
        .bg(rgba(with_alpha(theme.panel_high, 0xd4)))
        .child(content_header(
            theme,
            panel.active_section,
            &shortcuts.voice_interaction_label(),
            compact,
            cx,
        ))
        .child({
            let agents_section = panel.active_section == SettingsSection::Agents;
            div()
                .id("settings-content-scroll")
                .debug_selector(|| "settings-content-scroll".to_string())
                .flex_1()
                .min_w(px(0.0))
                .min_h(px(0.0))
                .overflow_x_hidden()
                .when(agents_section, |this| {
                    this.flex().flex_col().overflow_y_hidden()
                })
                .when(!agents_section, |this| {
                    this.overflow_y_scroll().track_scroll(content_scroll)
                })
                .px(px(padding))
                .pt(px(16.0))
                .pb(px(padding))
                .child(match panel.active_section {
                    SettingsSection::Themes => {
                        theme_appearance(theme_id, theme, width - padding * 2.0, cx)
                    }
                    SettingsSection::Keyboard => keyboard_settings(theme, shortcuts, cx),
                    SettingsSection::Voice => voice_settings(
                        crate::settings::voice::VoiceSettingsView {
                            theme,
                            state: voice.state,
                            inputs: voice.inputs,
                            content_scroll,
                            setup_scroll_target: voice.setup_scroll_target,
                            voice_shortcut_label: &shortcuts.voice_interaction_label(),
                        },
                        window,
                        cx,
                    ),
                    SettingsSection::Agents => agents_settings(
                        theme,
                        agent_definitions,
                        selected_agent_on_canvas,
                        agent_on_canvas_ids,
                        agent_list_scroll,
                        window,
                        cx,
                    ),
                    SettingsSection::Orchestration => {
                        orchestration_settings(theme, orchestration, cx)
                    }
                    SettingsSection::Wallpaper => {
                        wallpaper_settings(theme, wallpaper_state, wallpaper_previews, cx)
                    }
                    SettingsSection::Performance => performance_settings(theme, performance, cx),
                    SettingsSection::Mobile => {
                        crate::settings::mobile::settings(theme, &mobile, cx)
                    }
                    // Mock-only sections are hidden from the sidebar; keep a blank
                    // fallback so stale state or tests cannot route to old placeholders.
                    #[cfg(test)]
                    SettingsSection::Runtime | SettingsSection::Skills => div(),
                })
        })
}

fn content_header(
    theme: Theme,
    section: SettingsSection,
    voice_shortcut_label: &str,
    compact: bool,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let subtitle = content_subtitle(section, voice_shortcut_label);
    let mut title_column = div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(3.0))
        .child(
            div()
                .relative()
                .top(px(2.0))
                .text_size(px(if compact { 20.0 } else { 24.0 }))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(theme.text_primary))
                .child(content_title(section)),
        );
    if !subtitle.is_empty() {
        title_column = title_column.child(
            div()
                .text_size(px(15.75))
                .when(compact, |this| this.text_size(px(13.0)))
                .text_color(rgba(theme.text_tertiary))
                .child(subtitle),
        );
    }

    div()
        .min_h(px(119.0))
        .flex_none()
        .when(compact, |this| this.min_h(px(88.0)))
        .flex()
        .flex_row()
        .items_start()
        .justify_between()
        .gap(px(12.0))
        .px(px(if compact { 16.0 } else { 28.0 }))
        .py(px(if compact { 16.0 } else { 24.0 }))
        .border_b_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x24)))
        .child(title_column)
        .child(
            div()
                .id("settings-close")
                .debug_selector(|| "settings-close".to_string())
                .w(px(34.0))
                .h(px(34.0))
                .flex_none()
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
        SettingsSection::Orchestration => "Orchestration",
        SettingsSection::Wallpaper => "Wallpaper",
        SettingsSection::Performance => "Performance",
        SettingsSection::Mobile => "Mobile Connection",
    }
}

fn content_subtitle(section: SettingsSection, voice_shortcut_label: &str) -> String {
    match section {
        SettingsSection::Mobile => {
            "Connect your iPhone over a local network or Tailscale".to_string()
        }
        SettingsSection::Themes => "Choose the appearance of your workspace".to_string(),
        SettingsSection::Keyboard => {
            "Customize app commands and review system input behavior".to_string()
        }
        #[cfg(test)]
        SettingsSection::Runtime => "Configure shell behavior and live sessions".to_string(),
        SettingsSection::Voice => format!("Use {voice_shortcut_label} to talk to Norma"),
        SettingsSection::Agents => "Create, organize, and launch project agents".to_string(),
        #[cfg(test)]
        SettingsSection::Skills => "Manage installed skills and project skill sources".to_string(),
        SettingsSection::Orchestration => {
            "Install Skill + CLI so agents can delegate tasks through Norma OS".to_string()
        }
        SettingsSection::Wallpaper => "Choose a background".to_string(),
        SettingsSection::Performance => {
            "Balance animation responsiveness and idle energy use".to_string()
        }
    }
}
