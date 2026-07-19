use super::VoiceSettingsState;

#[derive(Clone)]
pub(crate) struct TtsActivationRollback {
    tts_model_id: Option<tts::TtsModelId>,
    tts_voice_id: Option<tts::TtsVoiceId>,
    active_provider_id: Option<tts::TtsProviderId>,
    minimax_model_id: Option<tts::TtsModelId>,
    minimax_voice_id: Option<tts::TtsVoiceId>,
    minimax_provenance: Option<tts::VoiceSelectionProvenance>,
    minimax_validated_credential_ref: Option<String>,
    minimax_validated_model_id: Option<tts::TtsModelId>,
    minimax_validated_voice_id: Option<tts::TtsVoiceId>,
    minimax_validated_selection: Option<tts::TtsValidatedSelection>,
    minimax_profile_generation: u64,
}

#[derive(Clone)]
pub(super) struct TtsPickerDraft {
    provider_id: tts::TtsProviderId,
    local_model_id: Option<tts::TtsModelId>,
    local_voice_id: Option<tts::TtsVoiceId>,
    minimax_model_id: Option<tts::TtsModelId>,
    minimax_voice_id: Option<tts::TtsVoiceId>,
    minimax_provenance: tts::VoiceSelectionProvenance,
    minimax_validated_selection: Option<tts::TtsValidatedSelection>,
    generation: u64,
    applying: bool,
    models_expanded: bool,
    connection_expanded: bool,
}

impl VoiceSettingsState {
    pub(crate) fn open_tts_picker(&mut self) {
        let provider_id = self
            .values
            .active_tts_provider_id()
            .cloned()
            .unwrap_or_else(local_provider_id);
        let connection_expanded = !self.minimax_credential_configured();
        self.tts_picker = Some(TtsPickerDraft {
            provider_id,
            local_model_id: self.values.tts_model_id().cloned(),
            local_voice_id: self.values.tts_voice_id().cloned(),
            minimax_model_id: self.values.minimax_tts_model_id().cloned(),
            minimax_voice_id: self.values.minimax_tts_voice_id().cloned(),
            minimax_provenance: self.values.minimax_tts_voice_provenance(),
            minimax_validated_selection: self.minimax_validated_selection.clone(),
            generation: self.minimax_profile_generation,
            applying: false,
            models_expanded: false,
            connection_expanded,
        });
    }

    pub(crate) fn cancel_tts_picker(&mut self) {
        self.discard_minimax_credential_candidate();
        self.tts_picker = None;
        self.cancel_edit();
    }

    pub(crate) fn tts_picker_open(&self) -> bool {
        self.tts_picker.is_some()
    }

    pub(crate) fn select_tts_picker_provider(&mut self, provider_id: &str) {
        let Some(draft) = self.tts_picker.as_mut() else {
            return;
        };
        if draft.applying {
            return;
        }
        let supported = matches!(
            provider_id,
            tts::LOCAL_TTS_PROVIDER_ID | tts::MINIMAX_TTS_PROVIDER_ID
        );
        if !supported {
            return;
        }
        if draft.provider_id.as_str() != provider_id {
            draft.applying = false;
        }
        draft.provider_id =
            tts::TtsProviderId::parse(provider_id).expect("built-in TTS Provider ID must be valid");
    }

    pub(crate) fn tts_picker_provider_id(&self) -> Option<&str> {
        self.tts_picker
            .as_ref()
            .map(|draft| draft.provider_id.as_str())
    }

    pub(crate) fn set_tts_picker_minimax_selection(
        &mut self,
        model_id: tts::TtsModelId,
        voice_id: tts::TtsVoiceId,
        provenance: tts::VoiceSelectionProvenance,
    ) {
        let Some(draft) = self.tts_picker.as_mut() else {
            return;
        };
        if draft.applying {
            return;
        }
        let changed = draft.minimax_model_id.as_ref() != Some(&model_id)
            || draft.minimax_voice_id.as_ref() != Some(&voice_id)
            || draft.minimax_provenance != provenance;
        if changed {
            draft.generation = draft.generation.wrapping_add(1);
            draft.minimax_validated_selection = None;
            draft.applying = false;
        }
        draft.minimax_model_id = Some(model_id);
        draft.minimax_voice_id = Some(voice_id);
        draft.minimax_provenance = provenance;
    }

    pub(crate) fn set_tts_picker_minimax_model(&mut self, model_id: tts::TtsModelId) {
        let Some(draft) = self.tts_picker.as_mut() else {
            return;
        };
        if draft.applying {
            return;
        }
        if draft.minimax_model_id.as_ref() != Some(&model_id) {
            draft.generation = draft.generation.wrapping_add(1);
            draft.minimax_validated_selection = None;
            draft.applying = false;
            draft.minimax_model_id = Some(model_id);
        }
    }

    pub(crate) fn tts_picker_minimax_model_id(&self) -> Option<&tts::TtsModelId> {
        self.tts_picker
            .as_ref()
            .and_then(|draft| draft.minimax_model_id.as_ref())
    }

    pub(crate) fn tts_picker_minimax_voice_id(&self) -> Option<&tts::TtsVoiceId> {
        self.tts_picker
            .as_ref()
            .and_then(|draft| draft.minimax_voice_id.as_ref())
    }

    pub(crate) fn tts_picker_minimax_provenance(&self) -> tts::VoiceSelectionProvenance {
        self.tts_picker
            .as_ref()
            .map(|draft| draft.minimax_provenance)
            .unwrap_or_else(|| self.values.minimax_tts_voice_provenance())
    }

    pub(crate) fn tts_picker_minimax_readiness(&self) -> tts::RemoteTtsReadiness {
        if !self.minimax_credential_configured() {
            return tts::RemoteTtsReadiness::Unconfigured;
        }
        let Some(profile) = self.tts_picker_minimax_profile() else {
            return tts::RemoteTtsReadiness::Configured;
        };
        let validation = tts::RemoteTtsValidation::new(
            self.minimax_account_status,
            self.values.minimax_tts_credential_ref().unwrap_or_default(),
            self.minimax_voice_catalog
                .voices()
                .iter()
                .map(|voice| voice.id().clone())
                .collect(),
            self.tts_picker
                .as_ref()
                .and_then(|draft| draft.minimax_validated_selection.clone()),
        );
        tts::evaluate_remote_readiness(Some(&profile), &validation)
    }

    pub(crate) fn commit_tts_picker_minimax_profile(&mut self) -> bool {
        if self.tts_picker_minimax_readiness() != tts::RemoteTtsReadiness::Ready {
            return false;
        }
        let Some(draft) = self.tts_picker.as_ref() else {
            return false;
        };
        let (Some(model_id), Some(voice_id)) = (
            draft.minimax_model_id.clone(),
            draft.minimax_voice_id.clone(),
        ) else {
            return false;
        };
        let provenance = draft.minimax_provenance;
        let manual_validated = provenance == tts::VoiceSelectionProvenance::Manual
            && draft.minimax_validated_selection.is_some();
        let picker = self.tts_picker.take();
        self.set_minimax_tts_selection(model_id, voice_id, provenance);
        if manual_validated {
            self.mark_minimax_manual_voice_validated(self.minimax_profile_generation);
        }
        self.tts_picker = picker;
        true
    }

    pub(crate) fn tts_picker_profile_generation(&self) -> Option<u64> {
        self.tts_picker.as_ref().map(|draft| draft.generation)
    }

    pub(crate) fn mark_tts_picker_minimax_validated(
        &mut self,
        expected_generation: u64,
    ) -> Option<bool> {
        let draft = self.tts_picker.as_mut()?;
        if draft.provider_id.as_str() != tts::MINIMAX_TTS_PROVIDER_ID {
            return None;
        }
        if draft.generation != expected_generation {
            return Some(false);
        }
        let (Some(model_id), Some(voice_id), Some(credential_ref)) = (
            draft.minimax_model_id.clone(),
            draft.minimax_voice_id.clone(),
            self.values
                .minimax_tts_credential_ref()
                .map(ToOwned::to_owned),
        ) else {
            return Some(false);
        };
        let profile = tts::TtsProviderProfile::new(
            minimax_provider_id(),
            model_id,
            voice_id,
            draft.minimax_provenance,
        );
        draft.minimax_validated_selection = Some(tts::TtsValidatedSelection::from_profile(
            &credential_ref,
            &profile,
        ));
        Some(true)
    }

    pub(crate) fn set_tts_picker_local_selection(
        &mut self,
        model_id: tts::TtsModelId,
        voice_id: tts::TtsVoiceId,
    ) {
        let Some(draft) = self.tts_picker.as_mut() else {
            return;
        };
        if draft.applying {
            return;
        }
        if draft.local_model_id.as_ref() != Some(&model_id)
            || draft.local_voice_id.as_ref() != Some(&voice_id)
        {
            draft.applying = false;
        }
        draft.local_model_id = Some(model_id);
        draft.local_voice_id = Some(voice_id);
    }

    pub(crate) fn begin_tts_picker_apply(&mut self) {
        if let Some(draft) = self.tts_picker.as_mut() {
            draft.applying = true;
        }
    }

    pub(crate) fn fail_tts_picker_apply(&mut self, message: impl Into<String>) {
        if let Some(draft) = self.tts_picker.as_mut() {
            draft.applying = false;
        }
        self.report_tts_status(message, true);
    }

    pub(crate) fn tts_picker_apply_pending(&self) -> bool {
        self.tts_picker.as_ref().is_some_and(|draft| draft.applying)
    }

    pub(crate) fn tts_picker_accepts_activation(
        &self,
        provider_id: &tts::TtsProviderId,
        model_id: &tts::TtsModelId,
        voice_id: &tts::TtsVoiceId,
    ) -> bool {
        let Some(draft) = self.tts_picker.as_ref().filter(|draft| draft.applying) else {
            return false;
        };
        if draft.provider_id != *provider_id {
            return false;
        }
        match provider_id.as_str() {
            tts::LOCAL_TTS_PROVIDER_ID => {
                draft.local_model_id.as_ref() == Some(model_id)
                    && draft.local_voice_id.as_ref() == Some(voice_id)
            }
            tts::MINIMAX_TTS_PROVIDER_ID => {
                draft.minimax_model_id.as_ref() == Some(model_id)
                    && draft.minimax_voice_id.as_ref() == Some(voice_id)
            }
            _ => false,
        }
    }

    pub(crate) fn tts_picker_ready(&self) -> bool {
        match self.tts_picker_provider_id() {
            Some(tts::LOCAL_TTS_PROVIDER_ID) => {
                let (Some(model_id), Some(voice_id)) = (
                    self.tts_picker_local_model_id(),
                    self.tts_picker_local_voice_id(),
                ) else {
                    return false;
                };
                let model_has_voice = tts::builtin_model_catalog()
                    .find(model_id)
                    .is_some_and(|model| model.voice(voice_id).is_some());
                let reference_ready =
                    tts::builtin_model_catalog()
                        .find(model_id)
                        .is_some_and(|model| {
                            !model.capabilities().requires_voice_reference()
                                || self.zipvoice_reference_ready()
                        });
                model_has_voice
                    && reference_ready
                    && matches!(
                        self.tts_model_manager.installation(model_id),
                        Some(super::super::tts_models::TtsModelInstallState::Ready)
                    )
            }
            Some(tts::MINIMAX_TTS_PROVIDER_ID) => {
                self.tts_picker_minimax_readiness() == tts::RemoteTtsReadiness::Ready
            }
            _ => false,
        }
    }

    pub(crate) fn finish_tts_picker(&mut self) {
        self.discard_minimax_credential_candidate();
        self.tts_picker = None;
        self.cancel_edit();
    }

    pub(crate) fn capture_tts_activation_rollback(&self) -> TtsActivationRollback {
        TtsActivationRollback {
            tts_model_id: self.saved.tts_model_id.clone(),
            tts_voice_id: self.saved.tts_voice_id.clone(),
            active_provider_id: self.saved.active_tts_provider_id.clone(),
            minimax_model_id: self.saved.minimax_tts_model_id.clone(),
            minimax_voice_id: self.saved.minimax_tts_voice_id.clone(),
            minimax_provenance: self.saved.minimax_tts_voice_provenance,
            minimax_validated_credential_ref: self.saved.minimax_validated_credential_ref.clone(),
            minimax_validated_model_id: self.saved.minimax_validated_model_id.clone(),
            minimax_validated_voice_id: self.saved.minimax_validated_voice_id.clone(),
            minimax_validated_selection: self.minimax_validated_selection.clone(),
            minimax_profile_generation: self.minimax_profile_generation,
        }
    }

    pub(crate) fn restore_tts_activation(&mut self, rollback: TtsActivationRollback) {
        self.saved.tts_model_id = rollback.tts_model_id;
        self.saved.tts_voice_id = rollback.tts_voice_id;
        self.saved.active_tts_provider_id = rollback.active_provider_id;
        self.saved.minimax_tts_model_id = rollback.minimax_model_id;
        self.saved.minimax_tts_voice_id = rollback.minimax_voice_id;
        self.saved.minimax_tts_voice_provenance = rollback.minimax_provenance;
        self.saved.minimax_validated_credential_ref = rollback.minimax_validated_credential_ref;
        self.saved.minimax_validated_model_id = rollback.minimax_validated_model_id;
        self.saved.minimax_validated_voice_id = rollback.minimax_validated_voice_id;
        self.minimax_validated_selection = rollback.minimax_validated_selection;
        self.minimax_profile_generation = rollback.minimax_profile_generation;
        self.refresh_values();
    }

    pub(crate) fn expand_tts_picker_models(&mut self) {
        if let Some(draft) = self.tts_picker.as_mut() {
            draft.models_expanded = true;
        }
    }

    pub(crate) fn tts_picker_models_expanded(&self) -> bool {
        self.tts_picker
            .as_ref()
            .is_some_and(|draft| draft.models_expanded)
    }

    pub(crate) fn toggle_tts_picker_connection(&mut self) {
        if let Some(draft) = self.tts_picker.as_mut() {
            draft.connection_expanded = !draft.connection_expanded;
        }
    }

    pub(crate) fn tts_picker_connection_expanded(&self) -> bool {
        self.tts_picker
            .as_ref()
            .is_some_and(|draft| draft.connection_expanded)
    }

    pub(crate) fn tts_picker_local_model_id(&self) -> Option<&tts::TtsModelId> {
        self.tts_picker
            .as_ref()
            .and_then(|draft| draft.local_model_id.as_ref())
    }

    pub(crate) fn tts_picker_local_voice_id(&self) -> Option<&tts::TtsVoiceId> {
        self.tts_picker
            .as_ref()
            .and_then(|draft| draft.local_voice_id.as_ref())
    }

    fn tts_picker_minimax_profile(&self) -> Option<tts::TtsProviderProfile> {
        let draft = self.tts_picker.as_ref()?;
        Some(tts::TtsProviderProfile::new(
            minimax_provider_id(),
            draft.minimax_model_id.clone()?,
            draft.minimax_voice_id.clone()?,
            draft.minimax_provenance,
        ))
    }
}

fn local_provider_id() -> tts::TtsProviderId {
    tts::TtsProviderId::parse(tts::LOCAL_TTS_PROVIDER_ID)
        .expect("built-in On-device TTS Provider ID must be valid")
}

fn minimax_provider_id() -> tts::TtsProviderId {
    tts::TtsProviderId::parse(tts::MINIMAX_TTS_PROVIDER_ID)
        .expect("built-in MiniMax TTS Provider ID must be valid")
}
