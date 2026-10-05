use crate::app::dialogue::SelectedTtsModel;

pub(super) fn selected_minimax_tts(
    state: &crate::settings::voice::VoiceSettingsState,
) -> Option<SelectedTtsModel> {
    Some(SelectedTtsModel::minimax(
        state
            .tts_picker_minimax_model_id()
            .or_else(|| state.values().minimax_tts_model_id())?
            .clone(),
        state
            .tts_picker_minimax_voice_id()
            .or_else(|| state.values().minimax_tts_voice_id())?
            .clone(),
        state.values().minimax_tts_credential_ref()?.to_string(),
        state.values().minimax_tts_api_key()?.to_string(),
    ))
}

pub(super) fn minimax_provider_id() -> tts::TtsProviderId {
    tts::TtsProviderId::parse(tts::MINIMAX_TTS_PROVIDER_ID)
        .expect("built-in MiniMax Provider ID must be valid")
}

pub(super) fn minimax_turbo_model_id() -> tts::TtsModelId {
    tts::TtsModelId::parse(tts::MINIMAX_SPEECH_TURBO_MODEL_ID)
        .expect("curated MiniMax Turbo model ID must be valid")
}

pub(super) fn query_minimax_voice_catalog(
    api_key: String,
) -> Result<tts::ProviderVoiceCatalog, tts::TtsError> {
    #[cfg(feature = "e2e-test-support")]
    if crate::settings::voice::tts_models::e2e_fake_tts_transport_enabled() {
        let voice_id = tts::TtsVoiceId::parse("system_voice_e2e")
            .expect("e2e MiniMax System Voice ID must be valid");
        return Ok(tts::ProviderVoiceCatalog::new(vec![
            tts::ProviderVoiceDescriptor::new(
                voice_id,
                ui::i18n::text("voice.system_narrator"),
                tts::TtsVoiceOrigin::System,
            ),
        ]));
    }
    tts::query_builtin_provider_voices(&minimax_provider_id(), api_key)
}
