use crate::app::{settings_crossfade_opacities, FeatureGuideSettingsTransition, MultisoulApp};
use gpui::{
    div, prelude::*, px, rgba, Animation, AnimationExt, AnyElement, Context, KeyDownEvent,
    MouseButton, ScrollHandle, Window,
};

#[path = "content.rs"]
mod content;
#[path = "navigation.rs"]
mod navigation;
#[path = "panel_style.rs"]
mod panel_style;

use self::content::{settings_content, SettingsContentContext, VoiceSettingsContent};
pub(crate) use self::navigation::SettingsSection;
use self::navigation::{settings_navigation, SettingsNavigationMode};
use self::panel_style::{settings_shadow, with_alpha};

/// The panel takes 70% of each viewport axis, within readable desktop bounds.
const PANEL_VIEWPORT_FRACTION: f32 = 0.7;
const PANEL_MIN_W: f32 = 760.0;
const PANEL_MIN_H: f32 = 520.0;
const PANEL_MAX_W: f32 = 1280.0;
const PANEL_MAX_H: f32 = 820.0;
const SIDEBAR_MIN_W: f32 = 160.0;
const SIDEBAR_MAX_W: f32 = 286.0;
const COMPACT_SIDEBAR_MIN_W: f32 = 144.0;
const COMPACT_SIDEBAR_MAX_W: f32 = 188.0;
const COMPACT_SIDEBAR_BREAKPOINT: f32 = 760.0;
const TOP_BAR_BREAKPOINT: f32 = 580.0;
const TOP_BAR_NAV_HEIGHT: f32 = 84.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct SettingsPanelLayout {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    sidebar_width: f32,
    navigation_mode: SettingsNavigationMode,
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
        let panel_width = (width * PANEL_VIEWPORT_FRACTION)
            .clamp(PANEL_MIN_W, PANEL_MAX_W)
            .min(available_width);
        let panel_height = (height * PANEL_VIEWPORT_FRACTION)
            .clamp(PANEL_MIN_H, PANEL_MAX_H)
            .min(available_height);
        let navigation_mode = if panel_width < TOP_BAR_BREAKPOINT {
            SettingsNavigationMode::TopBar
        } else if panel_width < COMPACT_SIDEBAR_BREAKPOINT {
            SettingsNavigationMode::CompactSidebar
        } else {
            SettingsNavigationMode::Sidebar
        };
        let sidebar_width = match navigation_mode {
            SettingsNavigationMode::Sidebar => (panel_width * 0.28)
                .clamp(SIDEBAR_MIN_W, SIDEBAR_MAX_W)
                .min((panel_width * 0.45).max(1.0)),
            SettingsNavigationMode::CompactSidebar => (panel_width * 0.26)
                .clamp(COMPACT_SIDEBAR_MIN_W, COMPACT_SIDEBAR_MAX_W)
                .min((panel_width * 0.38).max(1.0)),
            SettingsNavigationMode::TopBar => 0.0,
        };

        Self {
            left: ((width - panel_width) * 0.5).max(0.0),
            top: ((height - panel_height) * 0.5).max(0.0),
            width: panel_width,
            height: panel_height,
            sidebar_width,
            navigation_mode,
        }
    }

    fn content_width(self) -> f32 {
        match self.navigation_mode {
            SettingsNavigationMode::TopBar => self.width,
            SettingsNavigationMode::Sidebar | SettingsNavigationMode::CompactSidebar => {
                self.width - self.sidebar_width
            }
        }
    }

    fn content_height(self) -> f32 {
        match self.navigation_mode {
            SettingsNavigationMode::TopBar => (self.height - TOP_BAR_NAV_HEIGHT).max(0.0),
            SettingsNavigationMode::Sidebar | SettingsNavigationMode::CompactSidebar => self.height,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct HookInstallStatus {
    pub(crate) message: Option<ui::i18n::LocalizedText>,
    pub(crate) is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SettingsPanelState {
    pub(crate) open: bool,
    pub(crate) active_section: SettingsSection,
    pub(crate) codex_hook_status: HookInstallStatus,
    pub(crate) claude_hook_status: HookInstallStatus,
    pub(crate) cursor_hook_status: HookInstallStatus,
    pub(crate) grok_hook_status: HookInstallStatus,
    pub(crate) opencode_hook_status: HookInstallStatus,
}

#[derive(Debug, Default)]
pub(crate) struct SettingsViewportState {
    pub(crate) content_scroll: ScrollHandle,
    pub(crate) agent_list_scroll: ScrollHandle,
    pub(crate) pending_voice_setup_scroll: Option<crate::settings::voice::VoiceSetupDestination>,
}

impl Default for SettingsPanelState {
    fn default() -> Self {
        Self {
            open: false,
            active_section: SettingsSection::NormaPersona,
            codex_hook_status: HookInstallStatus::default(),
            claude_hook_status: HookInstallStatus::default(),
            cursor_hook_status: HookInstallStatus::default(),
            grok_hook_status: HookInstallStatus::default(),
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
        if self.agent_definitions.delete_dialog().is_some()
            && ev.keystroke.key == "escape"
            && !ev.keystroke.modifiers.modified()
        {
            self.agent_definitions.close_delete_dialog();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if self.handle_settings_text_key_down(ev, cx) {
            return;
        }
        if self.shortcuts.recording.is_some() {
            cx.stop_propagation();
            return;
        }
        // macOS skips text submission when KeyDown is marked handled. Let focused
        // child editors reach the platform text channel; the app skips canvas routing.
        if !self.focus_handle.is_focused(window)
            && crate::app::should_defer_keystroke_to_ime(&ev.keystroke)
        {
            tracing::debug!(target: "norma::input", route = "platform_text",
                "settings editor deferred character input");
            return;
        }
        if self.voice_settings_inputs.is_focused(window, cx) {
            cx.stop_propagation();
            return;
        }
        if ev.keystroke.key == "escape" && !ev.keystroke.modifiers.modified() {
            self.close_settings_panel(cx);
        } else if self.shortcuts.command_for_keystroke(&ev.keystroke)
            == Some(crate::shortcuts::ShortcutCommandId::ToggleSettings)
        {
            self.on_toggle_settings(&crate::app::ToggleSettings, window, cx);
        }
        // The panel may be rendered by a desktop host; do not route its keys to the canvas.
        tracing::debug!(target: "norma::input", route = "settings_consumed",
            panel_focused = self.focus_handle.is_focused(window),
            "settings key routing completed");
        cx.stop_propagation();
    }

    pub(crate) fn render_settings_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.ensure_norma_memory(window, cx);
        self.ensure_norma_persona(window, cx);
        let theme = self.theme;
        let win = window.viewport_size();
        let win_w = f32::from(win.width);
        let win_h = f32::from(win.height);
        let layout = SettingsPanelLayout::for_viewport(win_w, win_h);
        debug_assert!(
            layout.content_width() > 0.0,
            "settings layout must reserve positive content width"
        );
        debug_assert!(
            layout.content_height() > 0.0,
            "settings layout must reserve positive content height"
        );
        let navigation = settings_navigation(
            theme,
            self.settings_panel.active_section,
            layout.navigation_mode,
            layout.sidebar_width,
            cx,
        );
        let setup_scroll_target = self.settings_viewport.pending_voice_setup_scroll.take();
        let selected_agent_on_canvas = self
            .agent_definitions
            .draft()
            .id
            .is_some_and(|id| self.runtime.has_active_agent_definition(id));
        let agent_on_canvas_ids = self.runtime.active_agent_definition_ids();
        let brain_view = self.brain_settings_view();
        let content = settings_content(
            SettingsContentContext {
                width: layout.content_width() - 2.0, // Exclude the panel border.
                height: layout.content_height() - 2.0,
                theme,
                theme_id: self.theme_id,
                panel: &self.settings_panel,
                agent_definitions: &self.agent_definitions,
                selected_agent_on_canvas,
                agent_on_canvas_ids: &agent_on_canvas_ids,
                orchestration: &self.orchestration_settings,
                performance: &self.performance_settings,
                voice: VoiceSettingsContent {
                    live_call: self
                        .gpt_live
                        .view_for_project(self.runtime.workspace_root()),
                    state: &self.voice_settings,
                    inputs: &self.voice_settings_inputs,
                    setup_scroll_target,
                },
                content_scroll: &self.settings_viewport.content_scroll,
                agent_list_scroll: &self.settings_viewport.agent_list_scroll,
                wallpaper_state: &self.wallpaper_settings,
                wallpaper_previews: &mut self.wallpaper_previews,
                shortcuts: &self.shortcuts,
                mobile: self.mobile.clone(),
                brain: brain_view,
                persona: self
                    .norma_persona
                    .as_ref()
                    .expect("persona settings initialized")
                    .clone(),
                memory: self
                    .norma_memory
                    .as_ref()
                    .expect("memory settings initialized")
                    .clone(),
            },
            window,
            cx,
        );
        let (navigation, content) = match layout.navigation_mode {
            SettingsNavigationMode::TopBar => {
                (navigation.rounded_t(px(24.0)), content.rounded_b(px(24.0)))
            }
            SettingsNavigationMode::Sidebar | SettingsNavigationMode::CompactSidebar => {
                (navigation.rounded_l(px(24.0)), content.rounded_r(px(24.0)))
            }
        };

        // overlay 挡住自动聚焦；点击时显式 focus。
        let overlay = div()
            .id("settings-overlay")
            .debug_selector(|| "settings-overlay".to_string())
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .occlude()
            .bg(rgba(with_alpha(theme.graphite, 0x34)))
            .when(self.motion.reduce_motion, |overlay| {
                overlay.debug_selector(|| "settings-reduced-motion-crossfade".to_string())
            })
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
                    .when(
                        layout.navigation_mode == SettingsNavigationMode::TopBar,
                        |panel| panel.flex_col(),
                    )
                    .when(
                        layout.navigation_mode != SettingsNavigationMode::TopBar,
                        |panel| panel.flex_row(),
                    )
                    .overflow_hidden()
                    .rounded(px(24.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.node_stroke, 0xd8)))
                    .shadow(settings_shadow(theme))
                    .bg(rgba(with_alpha(theme.panel, 0xf2)))
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
                    .on_action(cx.listener(|app, _: &crate::app::ExitFocus, _, cx| {
                        app.close_settings_panel(cx);
                        cx.stop_propagation();
                    }))
                    .on_action(cx.listener(Self::on_toggle_settings))
                    .child(navigation)
                    .child(content),
            );
        match self.feature_guide.settings_transition() {
            FeatureGuideSettingsTransition::Opening => overlay
                .with_animation(
                    (
                        "settings-feature-guide-opening",
                        self.feature_guide.settings_transition_generation(),
                    ),
                    Animation::new(self.motion.reduced_fade_duration()),
                    |overlay, progress| {
                        overlay.opacity(
                            settings_crossfade_opacities(
                                FeatureGuideSettingsTransition::Opening,
                                progress,
                            )
                            .1,
                        )
                    },
                )
                .into_any_element(),
            FeatureGuideSettingsTransition::Closing => overlay
                .with_animation(
                    (
                        "settings-feature-guide-closing",
                        self.feature_guide.settings_transition_generation(),
                    ),
                    Animation::new(self.motion.reduced_fade_duration()),
                    |overlay, progress| {
                        overlay.opacity(
                            settings_crossfade_opacities(
                                FeatureGuideSettingsTransition::Closing,
                                progress,
                            )
                            .1,
                        )
                    },
                )
                .into_any_element(),
            FeatureGuideSettingsTransition::None => overlay.into_any_element(),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/settings_panel_tests.rs"]
mod settings_panel_tests;

#[cfg(test)]
#[path = "../../tests/unit/settings_panel_layout_tests.rs"]
mod settings_panel_layout_tests;

#[cfg(test)]
#[path = "../../tests/unit/settings_panel_voice_tests.rs"]
mod settings_panel_tests_voice;
