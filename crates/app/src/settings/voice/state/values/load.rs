use super::*;
use storage::SettingsStore;

impl SavedVoiceSettings {
    pub(crate) fn load(store: &SettingsStore) -> storage::StorageResult<Self> {
        let legacy_ollama_endpoint = non_empty_option(store.get_string(KEY_OLLAMA_ENDPOINT)?);
        let legacy_openai_api_key = non_empty_option(store.get_string(KEY_OPENAI_API_KEY)?);
        let legacy_openai_model =
            non_empty_option(store.get_string(KEY_OPENAI_CARD_PLANNER_MODEL)?);
        let ollama_credential_ref =
            non_empty_option(store.get_string(KEY_AGENT_OLLAMA_CREDENTIAL_REF)?);
        let ollama_plaintext = non_empty_option(store.get_string(KEY_AGENT_OLLAMA_API_KEY)?);
        let openai_credential_ref =
            non_empty_option(store.get_string(KEY_AGENT_OPENAI_CREDENTIAL_REF)?);
        let openai_plaintext =
            non_empty_option(store.get_string(KEY_AGENT_OPENAI_API_KEY)?).or(legacy_openai_api_key);
        Ok(Self {
            interaction_mode: store
                .get_string(KEY_INTERACTION_MODE)?
                .as_deref()
                .and_then(VoiceInteractionMode::from_stored),
            dictation_trigger: store
                .get_string(KEY_SINGLE_VOICE_COMMAND_TRIGGER)?
                .or(store.get_string(LEGACY_KEY_DICTATION_TRIGGER)?)
                .as_deref()
                .and_then(DictationTrigger::from_stored),
            asr_model_id: non_empty_option(store.get_string(KEY_ASR_MODEL_ID)?)
                .and_then(|value| AsrModelId::parse(value).ok()),
            asr_provider: store
                .get_string(KEY_ASR_PROVIDER)?
                .as_deref()
                .and_then(asr::AsrProviderId::from_stored),
            azure_asr_region: non_empty_option(store.get_string(KEY_ASR_AZURE_REGION)?),
            azure_asr_locale: non_empty_option(store.get_string(KEY_ASR_AZURE_LOCALE)?),
            azure_asr_credential_ref: non_empty_option(
                store.get_string(KEY_ASR_AZURE_CREDENTIAL_REF)?,
            ),
            azure_asr_ready: store
                .get_string(KEY_ASR_AZURE_READY)?
                .as_deref()
                .and_then(stored_bool),
            tts_model_id: non_empty_option(store.get_string(KEY_TTS_MODEL_ID)?)
                .and_then(|value| TtsModelId::parse(value).ok()),
            tts_voice_id: non_empty_option(store.get_string(KEY_TTS_VOICE_ID)?)
                .and_then(|value| TtsVoiceId::parse(value).ok()),
            active_tts_provider_id: non_empty_option(store.get_string(KEY_TTS_ACTIVE_PROVIDER_ID)?)
                .and_then(|value| tts::TtsProviderId::parse(value).ok()),
            minimax_tts_model_id: non_empty_option(store.get_string(KEY_TTS_MINIMAX_MODEL_ID)?)
                .and_then(|value| TtsModelId::parse(value).ok()),
            minimax_tts_voice_id: non_empty_option(store.get_string(KEY_TTS_MINIMAX_VOICE_ID)?)
                .and_then(|value| TtsVoiceId::parse(value).ok()),
            minimax_tts_voice_provenance: non_empty_option(
                store.get_string(KEY_TTS_MINIMAX_VOICE_PROVENANCE)?,
            )
            .and_then(|value| match value.as_str() {
                "catalog" => Some(tts::VoiceSelectionProvenance::Catalog),
                "manual" => Some(tts::VoiceSelectionProvenance::Manual),
                _ => None,
            }),
            minimax_tts_credential_ref: non_empty_option(
                store.get_string(KEY_TTS_MINIMAX_CREDENTIAL_REF)?,
            ),
            minimax_tts_api_key: None,
            minimax_validated_credential_ref: non_empty_option(
                store.get_string(KEY_TTS_MINIMAX_VALIDATED_CREDENTIAL_REF)?,
            ),
            minimax_validated_model_id: non_empty_option(
                store.get_string(KEY_TTS_MINIMAX_VALIDATED_MODEL_ID)?,
            )
            .and_then(|value| TtsModelId::parse(value).ok()),
            minimax_validated_voice_id: non_empty_option(
                store.get_string(KEY_TTS_MINIMAX_VALIDATED_VOICE_ID)?,
            )
            .and_then(|value| TtsVoiceId::parse(value).ok()),
            agent_provider: store
                .get_string(KEY_AGENT_PROVIDER)?
                .as_deref()
                .and_then(AgentProviderKind::from_stored),
            ollama_profile: SavedProviderProfile {
                base_url: non_empty_option(store.get_string(KEY_AGENT_OLLAMA_BASE_URL)?)
                    .or_else(|| legacy_ollama_endpoint.clone()),
                api_key: ollama_credential_ref
                    .is_none()
                    .then(|| ollama_plaintext.clone())
                    .flatten(),
                legacy_api_key: ollama_credential_ref
                    .is_some()
                    .then_some(ollama_plaintext)
                    .flatten(),
                credential_ref: ollama_credential_ref,
                model: non_empty_option(store.get_string(KEY_AGENT_OLLAMA_MODEL)?),
            },
            openai_profile: SavedProviderProfile {
                base_url: non_empty_option(store.get_string(KEY_AGENT_OPENAI_BASE_URL)?),
                api_key: openai_credential_ref
                    .is_none()
                    .then(|| openai_plaintext.clone())
                    .flatten(),
                legacy_api_key: openai_credential_ref
                    .is_some()
                    .then_some(openai_plaintext)
                    .flatten(),
                credential_ref: openai_credential_ref,
                model: non_empty_option(store.get_string(KEY_AGENT_OPENAI_MODEL)?)
                    .or(legacy_openai_model),
            },
            ollama_endpoint: legacy_ollama_endpoint,
            planner_model: non_empty_option(store.get_string(KEY_OLLAMA_MODEL)?),
            dialogue_model: non_empty_option(store.get_string(KEY_DIALOGUE_MODEL)?),
            refine_model: non_empty_option(store.get_string(KEY_REFINE_MODEL)?),
            input_device_id: non_empty_option(store.get_string(KEY_INPUT_DEVICE_ID)?),
            trace_enabled: store
                .get_string(KEY_TRACE_ENABLED)?
                .as_deref()
                .and_then(stored_bool),
            agent_task_announcements_enabled: store
                .get_string(KEY_AGENT_TASK_ANNOUNCEMENTS_ENABLED)?
                .as_deref()
                .and_then(stored_bool),
            voice_window_inset_enabled: store
                .get_string(KEY_VOICE_WINDOW_INSET_ENABLED)?
                .as_deref()
                .and_then(stored_bool),
            setup_completed: store
                .get_string(KEY_VOICE_SETUP_COMPLETED)?
                .as_deref()
                .and_then(stored_bool),
        })
    }
}
