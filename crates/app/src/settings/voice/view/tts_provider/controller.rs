use crate::app::dialogue::SelectedTtsModel;
use crate::app::MultisoulApp;
use gpui::{ClickEvent, Context, Window};

impl MultisoulApp {
    pub(crate) fn on_voice_connect_minimax_tts(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_minimax_tts_validation(cx);
    }

    pub(crate) fn on_voice_clear_minimax_tts_credential(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.clear_minimax_credential();
        self.persist_voice_settings_in_background(cx);
        cx.notify();
    }

    pub(crate) fn validate_saved_minimax_tts(&mut self, cx: &mut Context<Self>) {
        if self.voice_settings.values().minimax_tts_api_key().is_some() {
            self.start_minimax_tts_validation(cx);
        }
    }

    pub(crate) fn on_voice_select_minimax_turbo(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_minimax_tts_model(tts::MINIMAX_SPEECH_TURBO_MODEL_ID, cx);
    }

    pub(crate) fn on_voice_select_minimax_hd(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_minimax_tts_model(tts::MINIMAX_SPEECH_HD_MODEL_ID, cx);
    }

    pub(crate) fn on_voice_select_minimax_catalog_voice(
        &mut self,
        voice_id: tts::TtsVoiceId,
        cx: &mut Context<Self>,
    ) {
        let model_id = self
            .voice_settings
            .values()
            .minimax_tts_model_id()
            .cloned()
            .unwrap_or_else(minimax_turbo_model_id);
        self.voice_settings.set_minimax_tts_selection(
            model_id,
            voice_id,
            tts::VoiceSelectionProvenance::Catalog,
        );
        self.voice_settings_inputs.sync_field_from_state(
            crate::settings::voice::VoiceSettingsField::MiniMaxVoiceId,
            &self.voice_settings,
            cx,
        );
        self.persist_voice_settings_in_background(cx);
        cx.notify();
    }

    pub(crate) fn on_voice_use_minimax_tts(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.voice_settings.minimax_readiness() != tts::RemoteTtsReadiness::Ready {
            self.voice_settings.report_tts_status(
                "MiniMax must be Ready before it can become the active TTS Provider.",
                true,
            );
            cx.notify();
            return;
        }
        if self.voice_settings.settings_persistence_pending() {
            self.voice_settings.report_tts_status(
                "Wait for MiniMax credentials and settings to finish saving before activation.",
                false,
            );
            cx.notify();
            return;
        }
        let Some(selected) = selected_minimax_tts(&self.voice_settings) else {
            self.voice_settings
                .report_tts_status("MiniMax configuration is incomplete.", true);
            cx.notify();
            return;
        };
        let previous_provider_id = self
            .voice_settings
            .values()
            .active_tts_provider_id()
            .cloned();
        self.voice_settings
            .select_tts_provider(Some(minimax_provider_id()));
        self.pending_tts_activation = Some(crate::app::dialogue::PendingTtsActivation {
            selected,
            previous_provider_id,
        });
        self.persist_voice_settings_in_background(cx);
        cx.notify();
    }

    pub(crate) fn on_voice_use_local_tts(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(model_id) = self.voice_settings.values().tts_model_id().cloned() else {
            self.voice_settings.report_tts_status(
                "Install and select an On-device model before activating it.",
                true,
            );
            cx.notify();
            return;
        };
        let voice_id = self.voice_settings.values().tts_voice_id().cloned();
        self.select_tts_model(model_id, voice_id, cx);
    }

    pub(crate) fn on_voice_preview_minimax_tts(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selected) = selected_minimax_tts(&self.voice_settings) else {
            self.voice_settings.report_tts_status(
                "Connect MiniMax, choose a model and enter a Voice ID before Preview.",
                true,
            );
            cx.notify();
            return;
        };
        let validate_manual = self.voice_settings.values().minimax_tts_voice_provenance()
            == tts::VoiceSelectionProvenance::Manual;
        self.preview_tts_selection_work(
            move |runtime, cancelled| {
                super::super::tts_catalog::synthesize_tts_preview(&runtime, &selected, cancelled)
            },
            validate_manual,
            cx,
        );
    }

    fn select_minimax_tts_model(&mut self, model_id: &str, cx: &mut Context<Self>) {
        let model_id =
            tts::TtsModelId::parse(model_id).expect("curated MiniMax model ID must be valid");
        self.voice_settings.set_minimax_tts_model(model_id);
        self.persist_voice_settings_in_background(cx);
        cx.notify();
    }

    fn start_minimax_tts_validation(&mut self, cx: &mut Context<Self>) {
        let (generation, api_key) = match self.voice_settings.begin_minimax_connection() {
            Ok(connection) => connection,
            Err(error) => {
                self.voice_settings.report_tts_status(error, true);
                cx.notify();
                return;
            }
        };
        let api_key_for_request = api_key.clone();
        let work = cx.background_executor().spawn(async move {
            tts::query_builtin_provider_voices(&minimax_provider_id(), api_key_for_request)
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok(catalog) => {
                        let accepted = app
                            .voice_settings
                            .record_minimax_connection_success(generation, api_key, catalog);
                        if accepted {
                            if app.voice_settings.values().minimax_tts_model_id().is_none() {
                                app.voice_settings
                                    .set_minimax_tts_model(minimax_turbo_model_id());
                            }
                            let credential_write_pending =
                                app.voice_settings.credential_persistence_pending();
                            app.persist_voice_settings_in_background(cx);
                            if !credential_write_pending {
                                app.configure_active_minimax_tts_if_ready(cx);
                            }
                        }
                    }
                    Err(error) => {
                        app.voice_settings.record_minimax_connection_failure(
                            generation,
                            error.kind(),
                            format!("MiniMax connection failed: {error}"),
                        );
                        let minimax_active = app
                            .voice_settings
                            .values()
                            .active_tts_provider_id()
                            .is_some_and(|provider| {
                                provider.as_str() == tts::MINIMAX_TTS_PROVIDER_ID
                            });
                        if minimax_active {
                            app.invalidate_tts_for_configuration_change(cx);
                            crate::app::dialogue::configure_selected_tts_model(None);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn configure_active_minimax_tts_if_ready(&mut self, cx: &mut Context<Self>) {
        let active = self
            .voice_settings
            .values()
            .active_tts_provider_id()
            .is_some_and(|provider| provider.as_str() == tts::MINIMAX_TTS_PROVIDER_ID);
        if !active || self.voice_settings.minimax_readiness() != tts::RemoteTtsReadiness::Ready {
            return;
        }
        if let Some(selected) = selected_minimax_tts(&self.voice_settings) {
            self.invalidate_tts_for_configuration_change(cx);
            crate::app::dialogue::configure_selected_tts_model(Some(selected));
        }
    }
}

fn selected_minimax_tts(
    state: &crate::settings::voice::VoiceSettingsState,
) -> Option<SelectedTtsModel> {
    Some(SelectedTtsModel::minimax(
        state.values().minimax_tts_model_id()?.clone(),
        state.values().minimax_tts_voice_id()?.clone(),
        state.values().minimax_tts_credential_ref()?.to_string(),
        state.values().minimax_tts_api_key()?.to_string(),
    ))
}

fn minimax_provider_id() -> tts::TtsProviderId {
    tts::TtsProviderId::parse(tts::MINIMAX_TTS_PROVIDER_ID)
        .expect("built-in MiniMax Provider ID must be valid")
}

fn minimax_turbo_model_id() -> tts::TtsModelId {
    tts::TtsModelId::parse(tts::MINIMAX_SPEECH_TURBO_MODEL_ID)
        .expect("curated MiniMax Turbo model ID must be valid")
}
