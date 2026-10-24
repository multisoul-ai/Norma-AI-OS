use super::{with_alpha, SettingsPanelState, SettingsSection};
use crate::app::MultisoulApp;
use crate::settings::agents::{agent_drawer_layer, agents_settings, AgentDefinitionsState};
use crate::settings::appearance::theme_appearance;
use crate::settings::orchestration::{orchestration_settings, OrchestrationSettingsState};
use crate::settings::performance::{performance_settings, PerformanceSettingsState};
use crate::settings::spacing::{PAGE_PADDING, PAGE_PADDING_COMPACT, SECTION_GAP};
use crate::settings::typography::{SettingsText, TextRole};
use crate::settings::voice::{
    voice_settings, VoiceSettingsInputs, VoiceSettingsState, VoiceSetupDestination,
};
use crate::settings::wallpaper::{wallpaper_settings, PreviewCache, WallpaperSettingsState};
use crate::shortcuts::ShortcutSettingsState;
use gpui::{div, prelude::*, px, rgba, Context, Div, ScrollHandle, Window};
use ui::{Theme, ThemeId};

mod keyboard;

use self::keyboard::keyboard_settings;

pub(super) struct VoiceSettingsContent<'a> {
    pub(super) live_call: crate::app::LiveCallView,
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
    pub(super) effects: crate::settings::effects::page::EffectsPageData,
    pub(super) wallpaper_previews: &'a mut PreviewCache,
    pub(super) shortcuts: &'a ShortcutSettingsState,
    pub(super) memory: gpui::Entity<crate::settings::norma::MemoryView>,
    pub(super) persona: gpui::Entity<crate::settings::norma::persona::PersonaView>,
    pub(super) mobile: gpui::Entity<crate::mobile::controller::MobileController>,
    pub(super) brain: crate::app::BrainSettingsView,
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
        effects,
        wallpaper_previews,
        shortcuts,
        mobile,
        memory,
        persona,
        brain,
    } = content;
    let compact = width < 600.0 || height < 640.0;
    let padding = if compact {
        PAGE_PADDING_COMPACT
    } else {
        PAGE_PADDING
    };
    let agent_drawer = (panel.active_section == SettingsSection::Agents)
        .then(|| {
            agent_drawer_layer(
                theme,
                agent_definitions,
                selected_agent_on_canvas,
                window,
                cx,
            )
        })
        .flatten();
    div()
        .relative()
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .h_full()
        .flex()
        .flex_col()
        .bg(rgba(theme.panel))
        .settings_text(TextRole::Body, theme)
        .child(content_header(
            theme,
            panel.active_section,
            &shortcuts.voice_interaction_label(),
            compact,
            cx,
        ))
        .child({
            let fixed_height_section = matches!(
                panel.active_section,
                SettingsSection::Agents
                    | SettingsSection::NormaMemory
                    | SettingsSection::NormaPersona
            );
            div()
                .id("settings-content-scroll")
                .debug_selector(|| "settings-content-scroll".to_string())
                .flex_1()
                .min_w(px(0.0))
                .min_h(px(0.0))
                .overflow_x_hidden()
                .when(fixed_height_section, |this| {
                    this.flex().flex_col().overflow_y_hidden()
                })
                .when(!fixed_height_section, |this| {
                    this.overflow_y_scroll().track_scroll(content_scroll)
                })
                .px(px(padding))
                .pt(px(SECTION_GAP))
                .pb(px(padding))
                .child(match panel.active_section {
                    SettingsSection::General => div()
                        .flex()
                        .flex_col()
                        .gap(px(SECTION_GAP))
                        .child(crate::settings::general::general_settings(theme, cx))
                        .child(performance_settings(theme, performance, cx)),
                    SettingsSection::Themes => {
                        theme_appearance(theme_id, theme, width - padding * 2.0, cx)
                    }
                    SettingsSection::Keyboard => keyboard_settings(theme, shortcuts, cx),
                    SettingsSection::NormaMemory => div().h_full().min_h(px(0.0)).child(memory),
                    SettingsSection::NormaPersona => div().h_full().min_h(px(0.0)).child(persona),
                    SettingsSection::NormaVoice => voice_settings(
                        crate::settings::voice::VoiceSettingsView {
                            theme,
                            state: voice.state,
                            live_call: voice.live_call,
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
                    SettingsSection::Effects => crate::settings::effects::effects_settings(
                        theme,
                        &effects,
                        width - padding * 2.0,
                        window,
                        cx,
                    ),
                    SettingsSection::NormaBrain => {
                        crate::settings::brain::settings(theme, &brain, cx)
                    }
                    SettingsSection::Effects => crate::settings::effects::effects_settings(
                        theme,
                        &effects,
                        width - padding * 2.0,
                        window,
                        cx,
                    ),
                    SettingsSection::Mobile => {
                        crate::settings::mobile::settings(theme, &mobile, cx)
                    }
                    // Mock-only sections are hidden from the sidebar; keep a blank
                    // fallback so stale state or tests cannot route to old placeholders.
                    #[cfg(test)]
                    SettingsSection::Runtime | SettingsSection::Skills => div(),
                })
        })
        .children(agent_drawer)
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
            column.child(div().settings_text(TextRole::Eyebrow, theme).child("Norma"))
        })
        .child(
            div()
                .relative()
                .top(px(2.0))
                .settings_text(TextRole::Title, theme)
                .child(content_title(section)),
        );
    if !subtitle.is_empty() {
        title_column = title_column.child(
            div()
                .settings_text(TextRole::Subtitle, theme)
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
        .px(px(if compact {
            PAGE_PADDING_COMPACT
        } else {
            PAGE_PADDING
        }))
        .py(px(if compact { PAGE_PADDING_COMPACT } else { 28.0 }))
        .when(section == SettingsSection::Wallpaper, |header| {
            header.min_h(px(84.0)).py(px(SECTION_GAP))
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
        SettingsSection::NormaBrain => ui::i18n::text("settings.brain_subtitle").to_string(),
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
        SettingsSection::Effects => ui::i18n::text("effects.subtitle").to_string(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/settings_norma_scroll_tests.rs"]
mod norma_scroll_tests;
