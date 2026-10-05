use crate::app::{
    FeatureGuidePresentation, FeatureGuideSettingsTransition, FeatureGuideTopic, MultisoulApp,
    ToggleSettings,
};
use crate::settings::panel::SettingsSection;
use crate::settings::voice::VoiceRequirementState;
use gpui::{ClickEvent, Context, Window};

impl MultisoulApp {
    pub(crate) fn select_settings_section(
        &mut self,
        section: SettingsSection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings_panel.active_section == SettingsSection::NormaVoice
            && section != SettingsSection::NormaVoice
        {
            self.finish_voice_settings_inputs(cx);
        }
        self.settings_panel.select(section);
        self.settings_viewport
            .content_scroll
            .set_offset(gpui::point(gpui::px(0.0), gpui::px(0.0)));
        if section == SettingsSection::NormaVoice {
            self.refresh_asr_model_states(cx);
            self.refresh_tts_model_states(cx);
            self.refresh_voice_microphone_requirement(cx);
        }
        if section == SettingsSection::Wallpaper {
            self.wallpaper_settings.scoped.edit_project = self
                .wallpaper_settings
                .scoped
                .context
                .as_ref()
                .is_some_and(|context| context.area.is_some());
            self.refresh_wallpaper_library(cx);
        }
        if section == SettingsSection::Mobile {
            self.mobile.update(cx, |host, cx| {
                host.request(crate::mobile::controller::Command::Reload, cx)
            });
        }
        if section == SettingsSection::Orchestration {
            self.reconcile_orchestration_integrations(cx);
        }
        if section == SettingsSection::NormaMemory {
            self.ensure_norma_memory(window, cx);
        }
        if section == SettingsSection::NormaPersona {
            self.ensure_norma_persona(window, cx);
        }
        cx.notify();
    }
    pub(crate) fn on_toolbar_settings(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_settings_panel(cx);
        if self.settings_panel.open {
            self.focus_handle.focus(window);
        }
    }

    pub(crate) fn on_toggle_settings(
        &mut self,
        _: &ToggleSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_settings_panel(cx);
        if self.settings_panel.open {
            self.focus_handle.focus(window);
        }
    }

    pub(crate) fn toggle_settings_panel(&mut self, cx: &mut Context<Self>) {
        let was_open = self.settings_panel.open;
        if was_open && self.settings_blocks_close(cx) {
            cx.notify();
            return;
        }
        if was_open && self.should_crossfade_back_to_feature_guide() {
            self.begin_feature_guide_settings_transition(
                FeatureGuideSettingsTransition::Closing,
                cx,
            );
            return;
        }
        self.settings_panel.toggle();
        if !was_open && self.settings_panel.active_section == SettingsSection::Wallpaper {
            self.wallpaper_settings.scoped.edit_project = self
                .wallpaper_settings
                .scoped
                .context
                .as_ref()
                .is_some_and(|context| context.area.is_some());
            self.refresh_wallpaper_library(cx);
        }
        if was_open {
            self.shortcuts.recording = None;
            self.finish_voice_settings_inputs(cx);
            self.clear_wallpaper_previews(cx);
        }
        self.sync_feature_guide_blockers(cx);
        cx.notify();
    }

    pub(crate) fn close_settings_panel(&mut self, cx: &mut Context<Self>) {
        let was_open = self.settings_panel.open;
        if was_open && self.settings_blocks_close(cx) {
            cx.notify();
            return;
        }
        if was_open && self.should_crossfade_back_to_feature_guide() {
            self.begin_feature_guide_settings_transition(
                FeatureGuideSettingsTransition::Closing,
                cx,
            );
            return;
        }
        self.finish_settings_close_after_feature_guide_crossfade(cx);
    }

    pub(crate) fn finish_settings_close_after_feature_guide_crossfade(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let was_open = self.settings_panel.open;
        self.settings_panel.close();
        self.shortcuts.recording = None;
        self.finish_voice_settings_inputs(cx);
        if was_open {
            self.clear_wallpaper_previews(cx);
        }
        self.sync_feature_guide_blockers(cx);
        cx.notify();
    }

    fn should_crossfade_back_to_feature_guide(&self) -> bool {
        self.motion.reduce_motion
            && self.feature_guide.topic == FeatureGuideTopic::AgentOrchestration
            && self.feature_guide.presentation == FeatureGuidePresentation::Suspended
    }

    fn settings_blocks_close(&mut self, cx: &mut Context<Self>) -> bool {
        if let Some(memory) = &self.norma_memory {
            if memory.read(cx).is_dirty(cx) {
                self.settings_panel.select(SettingsSection::NormaMemory);
                memory.update(cx, |memory, cx| {
                    memory.close_prompt = true;
                    cx.notify();
                });
                return true;
            }
        }
        if let Some(persona) = &self.norma_persona {
            if persona.read(cx).is_dirty(cx) {
                self.settings_panel.select(SettingsSection::NormaPersona);
                self.settings_viewport
                    .content_scroll
                    .set_offset(gpui::point(gpui::px(0.0), gpui::px(0.0)));
                persona.update(cx, |view, cx| {
                    view.close_prompt = true;
                    cx.notify();
                });
                return true;
            }
        }
        if self.settings_panel.active_section != SettingsSection::Agents
            || !self.agent_definitions.is_dirty()
        {
            return false;
        }
        self.agent_definitions
            .set_status(ui::i18n::LocalizedText::new(
                "settings.save_or_discard_changes_before_closing_settings",
            ));
        true
    }

    pub(crate) fn open_voice_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings_panel.open();
        self.sync_feature_guide_blockers(cx);
        self.settings_panel.select(SettingsSection::NormaVoice);
        self.focus_handle.focus(window);
        self.refresh_asr_model_states(cx);
        self.refresh_tts_model_states(cx);
        self.refresh_voice_microphone_requirement(cx);
        cx.notify();
    }

    pub(crate) fn refresh_voice_microphone_requirement(&mut self, cx: &mut Context<Self>) {
        let check = cx
            .background_executor()
            .spawn(async { voice_sherpa::microphone_authorization_status() });
        cx.spawn(async move |this, cx| {
            let result = check.await;
            let _ = this.update(cx, |app, cx| {
                let requirement = match result {
                    Ok(voice_sherpa::MicrophoneAuthorization::Authorized) => {
                        VoiceRequirementState::Ready
                    }
                    Ok(voice_sherpa::MicrophoneAuthorization::Denied)
                    | Ok(voice_sherpa::MicrophoneAuthorization::Restricted) => {
                        VoiceRequirementState::Failed
                    }
                    Ok(voice_sherpa::MicrophoneAuthorization::NotDetermined) => {
                        VoiceRequirementState::Missing
                    }
                    Err(_) => VoiceRequirementState::Failed,
                };
                app.voice_settings.set_microphone_requirement(requirement);
                app.complete_voice_setup_if_ready(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn request_voice_microphone_access(&mut self, cx: &mut Context<Self>) {
        self.voice_settings
            .set_microphone_requirement(VoiceRequirementState::Installing {
                completed: 0,
                total: 1,
            });
        let request = cx
            .background_executor()
            .spawn(async { voice_sherpa::request_microphone_permission() });
        cx.spawn(async move |this, cx| {
            let result = request.await;
            let _ = this.update(cx, |app, cx| {
                app.voice_settings
                    .set_microphone_requirement(if result.is_ok() {
                        VoiceRequirementState::Ready
                    } else {
                        VoiceRequirementState::Failed
                    });
                app.complete_voice_setup_if_ready(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn finish_voice_settings_inputs(&mut self, cx: &mut Context<Self>) {
        self.flush_voice_text_input_autosave(cx);
        self.voice_settings.set_reviewing_setup(false);
        if let Some(field) = self.voice_settings.editing_field() {
            self.voice_settings.cancel_edit_field(field);
        }
        self.voice_settings_inputs
            .sync_from_state(&self.voice_settings, cx);
    }

    fn clear_wallpaper_previews(&mut self, cx: &mut Context<Self>) {
        let released = self.wallpaper_previews.clear();
        crate::settings::wallpaper::preview::release_images(released, cx);
    }
}
