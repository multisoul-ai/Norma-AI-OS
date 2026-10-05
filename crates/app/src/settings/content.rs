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
use ui::component::StyledExt;
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
    pub(super) memory: gpui::Entity<crate::settings::norma::MemoryView>,
    pub(super) persona: gpui::Entity<crate::settings::norma::persona::PersonaView>,
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
        memory,
        persona,
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
        .bg(rgba(theme.panel))
        .child(content_header(
            theme,
            panel.active_section,
            &shortcuts.voice_interaction_label(),
            compact,
            cx,
        ))
        .child({
            let agents_section = panel.active_section == SettingsSection::Agents
                || panel.active_section == SettingsSection::NormaMemory;
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
                    SettingsSection::General => {
                        crate::settings::general::general_settings(theme, cx)
                    }
                    SettingsSection::Themes => {
                        theme_appearance(theme_id, theme, width - padding * 2.0, cx)
                    }
                    SettingsSection::Keyboard => keyboard_settings(theme, shortcuts, cx),
                    SettingsSection::NormaMemory => div().h_full().min_h(px(0.0)).child(memory),
                    SettingsSection::NormaPersona => div().child(persona),
                    SettingsSection::NormaVoice => voice_settings(
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
                    SettingsSection::Wallpaper => wallpaper_settings(
                        theme,
                        wallpaper_state,
                        wallpaper_previews,
                        width - padding * 2.0,
                        cx,
                    ),
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
        .when(section.is_norma(), |column| {
            column.child(
                div()
                    .text_size(px(11.0))
                    .font_medium()
                    .text_color(rgba(theme.text_secondary))
                    .child("Norma"),
            )
        })
        .child(
            div()
                .relative()
                .top(px(2.0))
                .text_size(px(if compact { 20.0 } else { 24.0 }))
                .font_weight(FontWeight::SEMIBOLD)
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
        .when(section.is_norma(), |this| this.min_h(px(104.0)))
        .when(compact, |this| this.min_h(px(88.0)))
        .flex()
        .flex_row()
        .items_start()
        .justify_between()
        .gap(px(12.0))
        .px(px(if compact { 16.0 } else { 28.0 }))
        .py(px(if compact { 16.0 } else { 24.0 }))
        .when(section == SettingsSection::Wallpaper, |header| {
            header.min_h(px(84.0)).py(px(18.0))
        })
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
                .child({
                    use ui::component::button::{Button, ButtonVariants};
                    Button::new("settings-close-button")
                        .ghost()
                        .w_full()
                        .h_full()
                        .tooltip(ui::i18n::text("settings.close_settings"))
                        .child("×")
                        .on_click(cx.listener(|app, _, _window, cx| app.close_settings_panel(cx)))
                }),
        )
}

fn content_title(section: SettingsSection) -> &'static str {
    section.title()
}

fn content_subtitle(section: SettingsSection, voice_shortcut_label: &str) -> String {
    match section {
        SettingsSection::General => ui::i18n::text("general.subtitle").into(),
        SettingsSection::Mobile => {
            ui::i18n::text("settings.connect_your_iphone_over_a_local_network_or_tailscale")
                .to_string()
        }
        SettingsSection::Themes => {
            ui::i18n::text("settings.choose_the_appearance_of_your_workspace").to_string()
        }
        SettingsSection::Keyboard => {
            ui::i18n::text("settings.customize_app_commands_and_review_system_input_behavior")
                .to_string()
        }
        #[cfg(test)]
        SettingsSection::Runtime => {
            ui::i18n::text("settings.configure_shell_behavior_and_live_sessions").to_string()
        }
        SettingsSection::NormaMemory => ui::i18n::text(
            "settings.review_and_organize_the_preferences_and_project_knowledge_norma_remembers",
        )
        .into(),
        SettingsSection::NormaVoice => ui::i18n::format(
            "settings.let_norma_hear_you_and_respond_voice_shortcut_label",
            &[("voice_shortcut_label", (voice_shortcut_label).to_string())],
        ),
        SettingsSection::NormaPersona => {
            ui::i18n::text("settings.make_every_conversation_feel_more_like_you").into()
        }
        SettingsSection::Agents => {
            ui::i18n::text("settings.create_organize_and_launch_project_agents").to_string()
        }
        #[cfg(test)]
        SettingsSection::Skills => {
            ui::i18n::text("settings.manage_installed_skills_and_project_skill_sources").to_string()
        }
        SettingsSection::Orchestration => ui::i18n::text(
            "settings.install_skill_cli_so_agents_can_delegate_tasks_through_norma_os",
        )
        .to_string(),
        SettingsSection::Wallpaper => String::new(),
        SettingsSection::Performance => {
            ui::i18n::text("settings.balance_animation_responsiveness_and_idle_energy_use")
                .to_string()
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/settings_norma_scroll_tests.rs"]
mod norma_scroll_tests;
