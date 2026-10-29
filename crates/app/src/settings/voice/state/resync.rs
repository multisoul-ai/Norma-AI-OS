//! 全局语音设置的跨实例重新同步：每个项目会话各持一份内存快照，需要能追上库中最新值。
use super::values::SavedVoiceSettings;
use super::VoiceSettingsState;

pub(super) fn minimax_validated_selection(
    saved: &SavedVoiceSettings,
) -> Option<tts::TtsValidatedSelection> {
    saved
        .minimax_validated_credential_ref
        .as_deref()
        .zip(saved.minimax_validated_model_id.as_ref())
        .zip(saved.minimax_validated_voice_id.as_ref())
        .map(|((credential_ref, model_id), voice_id)| {
            let provider_id = tts::TtsProviderId::parse(tts::MINIMAX_TTS_PROVIDER_ID)
                .expect("built-in MiniMax TTS Provider ID must be valid");
            let profile = tts::TtsProviderProfile::new(
                provider_id,
                model_id.clone(),
                voice_id.clone(),
                tts::VoiceSelectionProvenance::Manual,
            );
            tts::TtsValidatedSelection::from_profile(credential_ref, &profile)
        })
}

fn without_runtime_secrets(saved: &SavedVoiceSettings) -> SavedVoiceSettings {
    let mut saved = saved.clone();
    saved.ollama_profile.api_key = None;
    saved.openai_profile.api_key = None;
    saved.minimax_tts_api_key = None;
    saved
}

/// 智能体 provider 相关配置是否不同（决定是否需要重新校验连接）。
pub(crate) fn provider_configuration_differs(
    a: &SavedVoiceSettings,
    b: &SavedVoiceSettings,
) -> bool {
    a.agent_provider != b.agent_provider
        || a.ollama_profile != b.ollama_profile
        || a.openai_profile != b.openai_profile
        || a.ollama_endpoint != b.ollama_endpoint
}

/// TTS 相关配置是否不同（决定是否需要让对话运行时重新选择模型）。
pub(crate) fn tts_configuration_differs(a: &SavedVoiceSettings, b: &SavedVoiceSettings) -> bool {
    a.tts_model_id != b.tts_model_id
        || a.tts_voice_id != b.tts_voice_id
        || a.active_tts_provider_id != b.active_tts_provider_id
        || a.minimax_tts_model_id != b.minimax_tts_model_id
        || a.minimax_tts_voice_id != b.minimax_tts_voice_id
        || a.minimax_tts_voice_provenance != b.minimax_tts_voice_provenance
        || a.minimax_tts_credential_ref != b.minimax_tts_credential_ref
        || a.minimax_validated_credential_ref != b.minimax_validated_credential_ref
        || a.minimax_validated_model_id != b.minimax_validated_model_id
        || a.minimax_validated_voice_id != b.minimax_validated_voice_id
}

impl VoiceSettingsState {
    pub(crate) fn saved_snapshot(&self) -> SavedVoiceSettings {
        self.saved.clone()
    }

    /// 每次开始或结束一轮落库都会递增；后台读库期间发生过落库则读到的快照可能早于该写入。
    pub(crate) fn persistence_epoch(&self) -> u64 {
        self.persistence_epoch
    }

    pub(crate) fn begin_saved_snapshot_sync(&mut self) -> u64 {
        self.resync_generation = self.resync_generation.wrapping_add(1);
        self.resync_generation
    }

    /// 后台读取期间若本实例配置已变或发生过落库（用户刚改过），读到的快照可能已过期，直接丢弃。
    /// 比较时忽略钥匙串密钥：异步加载密钥不算配置变化。
    pub(crate) fn apply_saved_snapshot_if_unchanged(
        &mut self,
        baseline: &SavedVoiceSettings,
        epoch: u64,
        generation: u64,
        snapshot: SavedVoiceSettings,
        reload_credentials: bool,
    ) -> bool {
        if self.resync_generation != generation
            || self.persistence_epoch != epoch
            || without_runtime_secrets(&self.saved) != without_runtime_secrets(baseline)
        {
            return false;
        }
        self.apply_saved_snapshot(snapshot, reload_credentials)
    }

    /// 返回配置值是否发生变化。运行时状态（麦克风授权、模型扫描、编辑草稿）一律保留；
    /// 钥匙串密钥仅在引用未变且未要求重读时保留，避免每次切换项目都重新读钥匙串。
    pub(crate) fn apply_saved_snapshot(
        &mut self,
        mut snapshot: SavedVoiceSettings,
        reload_credentials: bool,
    ) -> bool {
        if self.settings_persistence_pending() || self.credential_persistence_pending() {
            return false;
        }
        if !reload_credentials {
            keep_loaded_secret(&mut snapshot.ollama_profile, &self.saved.ollama_profile);
            keep_loaded_secret(&mut snapshot.openai_profile, &self.saved.openai_profile);
        }
        if snapshot.minimax_tts_api_key.is_none()
            && snapshot.minimax_tts_credential_ref == self.saved.minimax_tts_credential_ref
        {
            snapshot.minimax_tts_api_key = self.saved.minimax_tts_api_key.clone();
        }
        if snapshot == self.saved {
            return false;
        }
        if snapshot.asr_provider != self.saved.asr_provider
            || snapshot.desktop_mode != self.saved.desktop_mode
            || snapshot.interaction_mode != self.saved.interaction_mode
        {
            self.invalidate_a1_preparation();
        }
        let provider_changed = provider_configuration_differs(&snapshot, &self.saved);
        self.minimax_validated_selection = minimax_validated_selection(&snapshot);
        self.setup_was_completed |= snapshot.setup_completed.unwrap_or(false);
        self.saved = snapshot;
        self.refresh_values();
        if provider_changed {
            self.invalidate_provider_requirement();
        }
        true
    }
}

fn keep_loaded_secret(
    snapshot: &mut super::provider_values::SavedProviderProfile,
    current: &super::provider_values::SavedProviderProfile,
) {
    if snapshot.api_key.is_none()
        && snapshot.credential_ref.is_some()
        && snapshot.credential_ref == current.credential_ref
    {
        snapshot.api_key = current.api_key.clone();
    }
}
