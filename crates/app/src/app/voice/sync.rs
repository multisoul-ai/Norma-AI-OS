//! 语音设置是全局的：每个项目会话各持一份内存副本，这里负责让它们追上库中最新值，
//! 并补齐后台创建的会话从未执行过的就绪检测，避免切换项目后反复配置。
use crate::app::MultisoulApp;
use crate::settings::voice::{
    provider_configuration_differs, tts_configuration_differs, SavedVoiceSettings,
    VoiceRequirement, VoiceRequirementState,
};
use gpui::Context;

impl MultisoulApp {
    /// 后台读取全局语音设置并合并。`refresh_runtime` 为真且本会话尚未做过就绪检测时，
    /// 补做麦克风/模型/凭据检测（每个会话仅一次，避免打断已有的安装进度与就绪状态）。
    pub(crate) fn resync_voice_settings(&mut self, refresh_runtime: bool, cx: &mut Context<Self>) {
        let Some(store) = self.voice_settings.credential_store() else {
            return;
        };
        let baseline = self.voice_settings.saved_snapshot();
        let epoch = self.voice_settings.persistence_epoch();
        let generation = self.voice_settings.begin_saved_snapshot_sync();
        let load = cx
            .background_executor()
            .spawn(async move { SavedVoiceSettings::load(&store) });
        cx.spawn(async move |this, cx| {
            let snapshot = load.await;
            let _ = this.update(cx, |app, cx| {
                let applied = snapshot.ok().and_then(|snapshot| {
                    let latest = snapshot.clone();
                    app.voice_settings
                        .apply_saved_snapshot_if_unchanged(
                            &baseline, epoch, generation, snapshot, false,
                        )
                        .then_some(latest)
                });
                if applied.is_some() {
                    if app.voice_settings.editing_field().is_none() {
                        app.voice_settings_inputs
                            .sync_from_state(&app.voice_settings, cx);
                    }
                    app.sync_voice_window_inset_enabled();
                }
                let first_runtime_check = refresh_runtime && !app.voice_runtime_checked;
                if first_runtime_check {
                    app.voice_runtime_checked = true;
                    app.refresh_voice_runtime_fully(cx);
                }
                if let Some(latest) = applied {
                    app.apply_changed_voice_settings(&baseline, &latest, first_runtime_check, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn refresh_voice_runtime_fully(&mut self, cx: &mut Context<Self>) {
        self.refresh_asr_model_states(cx);
        self.refresh_tts_model_states(cx);
        self.refresh_voice_microphone_requirement(cx);
        self.refresh_voice_provider_credentials(cx);
        if self
            .voice_settings
            .requirement_state(VoiceRequirement::AgentProvider)
            != VoiceRequirementState::Ready
        {
            self.validate_saved_voice_provider(cx);
        }
    }

    /// 其它会话改了配置后，让本会话的运行时（密钥、provider 校验、TTS）按新配置重建。
    fn apply_changed_voice_settings(
        &mut self,
        previous: &SavedVoiceSettings,
        latest: &SavedVoiceSettings,
        already_refreshed: bool,
        cx: &mut Context<Self>,
    ) {
        let mode_changed = previous.desktop_mode != latest.desktop_mode;
        if mode_changed
            || previous.gpt_live_interaction_mode != latest.gpt_live_interaction_mode
            || previous.gpt_live_profile.credential_ref()
                != latest.gpt_live_profile.credential_ref()
            || previous.input_device_id != latest.input_device_id
        {
            self.stop_gpt_live_voice(cx);
        }
        let asr_changed = previous.asr_provider != latest.asr_provider
            || previous.asr_model_id != latest.asr_model_id
            || previous.azure_asr_credential_ref != latest.azure_asr_credential_ref
            || previous.azure_asr_region != latest.azure_asr_region
            || previous.azure_asr_locale != latest.azure_asr_locale
            || previous.azure_asr_ready != latest.azure_asr_ready;
        if mode_changed
            || previous.doubao_interaction_mode != latest.doubao_interaction_mode
            || previous.doubao_credential_ref != latest.doubao_credential_ref
            || previous.doubao_speaker != latest.doubao_speaker
            || previous.input_device_id != latest.input_device_id
        {
            self.stop_doubao_voice(cx);
            self.cancel_agent_announcements();
        }
        if mode_changed
            || previous.interaction_mode != latest.interaction_mode
            || previous.dictation_trigger != latest.dictation_trigger
            || asr_changed
            || previous.input_device_id != latest.input_device_id
        {
            self.stop_continuous_listening(cx);
            self.voice.stop_for_model_switch();
        }
        let tts_changed = tts_configuration_differs(previous, latest);
        if tts_changed {
            self.cancel_agent_announcements();
            self.clear_agent_announcement_audio_for_voice_change();
            self.tts_preview.cancel();
            self.invalidate_tts_for_configuration_change(cx);
        }
        if already_refreshed {
            return;
        }
        if asr_changed {
            self.refresh_asr_model_states(cx);
        }
        self.refresh_voice_provider_credentials(cx);
        if provider_configuration_differs(previous, latest) {
            self.validate_saved_voice_provider(cx);
        }
        if tts_changed {
            let minimax_active = self
                .voice_settings
                .values()
                .active_tts_provider_id()
                .is_some_and(|provider| provider.as_str() == tts::MINIMAX_TTS_PROVIDER_ID);
            if minimax_active {
                self.configure_active_minimax_tts_if_ready(cx);
            } else {
                self.refresh_tts_model_states(cx);
            }
        }
    }

    /// 通知同库所有会话；模型工件变化时，即使设置值相同也须刷新 ASR/VAD 缓存。
    pub(crate) fn broadcast_voice_settings_changed(
        &mut self,
        refresh_asr_models: bool,
        cx: &mut Context<Self>,
    ) {
        let database = self.database_paths.app_db_path();
        let mut hosts = crate::workspace::desktop::sessions::hosts(database, cx);
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            hosts.push(host);
        }
        let mut sessions = crate::workspace::sessions::for_database(database, cx);
        sessions.extend(self.desktop_workspace.sessions.values().cloned());
        // 发起方正在被借用，不能读取它；收集完所有宿主的项目后再逐一更新。
        for host in hosts {
            if host.entity_id() != cx.entity_id() {
                sessions.extend(host.read(cx).desktop_workspace.sessions.values().cloned());
                sessions.push(host);
            }
        }
        let mut visited = std::collections::HashSet::from([cx.entity_id()]);
        for session in sessions {
            if visited.insert(session.entity_id()) {
                session.update(cx, |session, cx| {
                    session.resync_voice_settings(false, cx);
                    if refresh_asr_models {
                        session.refresh_asr_model_states(cx);
                    }
                });
            }
        }
        // 对端写入期间被跳过的快照不会重试，落库完成后让发起方自己也追一次。
        self.resync_voice_settings(false, cx);
        if refresh_asr_models {
            self.refresh_asr_model_states(cx);
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/voice_settings_sync_tests.rs"]
mod tests;
