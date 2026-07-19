use crate::app::dialogue::SelectedTtsModel;
use crate::app::MultisoulApp;
use gpui::{ClickEvent, Context, Window};

mod activation;
mod work;
use work::{minimax_turbo_model_id, query_minimax_voice_catalog, selected_minimax_tts};

impl MultisoulApp {
    pub(crate) fn on_voice_open_tts_picker(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.open_tts_picker();
        self.voice_settings_inputs
            .sync_from_state(&self.voice_settings, cx);
        cx.notify();
    }

    pub(crate) fn on_voice_cancel_tts_picker(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_tts_activation.is_some() {
            self.voice_settings.report_tts_status(
                "Wait for the selected Voice to finish saving before closing.",
                false,
            );
            cx.notify();
            return;
        }
        self.voice_settings.cancel_tts_picker();
        self.voice_settings_inputs
            .sync_from_state(&self.voice_settings, cx);
        cx.notify();
    }

    pub(crate) fn on_voice_pick_local_tts(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings
            .select_tts_picker_provider(tts::LOCAL_TTS_PROVIDER_ID);
        cx.notify();
    }

    pub(crate) fn on_voice_pick_minimax_tts(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings
            .select_tts_picker_provider(tts::MINIMAX_TTS_PROVIDER_ID);
        self.voice_settings_inputs
            .sync_from_state(&self.voice_settings, cx);
        cx.notify();
    }

    pub(crate) fn on_voice_pick_local_tts_voice(
        &mut self,
        model_id: tts::TtsModelId,
        voice_id: tts::TtsVoiceId,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings
            .set_tts_picker_local_selection(model_id, voice_id);
        cx.notify();
    }

    pub(crate) fn on_voice_expand_tts_models(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.expand_tts_picker_models();
        cx.notify();
    }

    pub(crate) fn on_voice_toggle_minimax_connection(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.toggle_tts_picker_connection();
        self.voice_settings_inputs
            .sync_from_state(&self.voice_settings, cx);
        cx.notify();
    }

    pub(crate) fn on_voice_connect_minimax_tts(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pending_minimax_preview_after_validation = None;
        self.start_minimax_tts_validation(false, cx);
    }

    pub(crate) fn on_voice_clear_minimax_tts_credential(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pending_minimax_preview_after_validation = None;
        self.voice_settings.clear_minimax_credential();
        self.persist_voice_settings_in_background(cx);
        cx.notify();
    }

    pub(crate) fn validate_saved_minimax_tts(&mut self, cx: &mut Context<Self>) {
        if self.voice_settings.values().minimax_tts_api_key().is_some() {
            self.start_minimax_tts_validation(false, cx);
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
            .tts_picker_minimax_model_id()
            .or_else(|| self.voice_settings.values().minimax_tts_model_id())
            .cloned()
            .unwrap_or_else(minimax_turbo_model_id);
        if self.voice_settings.tts_picker_provider_id() == Some(tts::MINIMAX_TTS_PROVIDER_ID) {
            self.voice_settings.set_tts_picker_minimax_selection(
                model_id,
                voice_id,
                tts::VoiceSelectionProvenance::Catalog,
            );
            self.voice_settings_inputs.sync_field_from_state(
                crate::settings::voice::VoiceSettingsField::MiniMaxVoiceId,
                &self.voice_settings,
                cx,
            );
            cx.notify();
            return;
        }
        let changed = self.voice_settings.set_minimax_tts_selection(
            model_id,
            voice_id,
            tts::VoiceSelectionProvenance::Catalog,
        );
        if changed {
            self.configure_active_minimax_tts_if_ready(cx);
        }
        self.voice_settings_inputs.sync_field_from_state(
            crate::settings::voice::VoiceSettingsField::MiniMaxVoiceId,
            &self.voice_settings,
            cx,
        );
        self.persist_voice_settings_in_background(cx);
        cx.notify();
    }

    pub(crate) fn preview_minimax_catalog_voice(
        &mut self,
        voice_id: tts::TtsVoiceId,
        cx: &mut Context<Self>,
    ) {
        let Some(model_id) = self
            .voice_settings
            .tts_picker_minimax_model_id()
            .or_else(|| self.voice_settings.values().minimax_tts_model_id())
            .cloned()
        else {
            self.voice_settings
                .report_tts_status("Choose a MiniMax model before Preview.", true);
            cx.notify();
            return;
        };
        let (Some(credential_ref), Some(api_key)) = (
            self.voice_settings
                .values()
                .minimax_tts_credential_ref()
                .map(ToOwned::to_owned),
            self.voice_settings
                .values()
                .minimax_tts_api_key()
                .map(ToOwned::to_owned),
        ) else {
            self.voice_settings
                .report_tts_status("Connect MiniMax before Preview.", true);
            cx.notify();
            return;
        };
        let selected = SelectedTtsModel::minimax(model_id, voice_id, credential_ref, api_key);
        self.preview_tts_selection_work(
            move |runtime, cancelled| {
                super::super::tts_catalog::synthesize_tts_preview(&runtime, &selected, cancelled)
            },
            false,
            true,
            cx,
        );
    }

    pub(crate) fn on_voice_preview_minimax_tts(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.voice_settings.settings_persistence_pending()
            || self.voice_settings.minimax_persistence_blocks_preview()
        {
            self.voice_settings.report_tts_status(
                "Wait for the current MiniMax credential or validation proof to finish saving.",
                false,
            );
            cx.notify();
            return;
        }
        let needs_connection = self.voice_settings.minimax_account_status()
            != tts::RemoteAccountStatus::Ready
            || self.voice_settings.minimax_credential_draft_pending()
            || self
                .voice_settings
                .values()
                .minimax_tts_credential_ref()
                .is_none();
        if needs_connection {
            self.pending_minimax_preview_after_validation = None;
            self.voice_settings.report_tts_status(
                "Connect MiniMax before Preview. Preview never saves credentials.",
                true,
            );
            cx.notify();
            return;
        }
        self.start_minimax_tts_preview(cx);
    }

    pub(crate) fn start_minimax_tts_preview(&mut self, cx: &mut Context<Self>) {
        let Some(selected) = selected_minimax_tts(&self.voice_settings) else {
            self.voice_settings.report_tts_status(
                "Connect MiniMax, choose a model and enter a Voice ID before Preview.",
                true,
            );
            cx.notify();
            return;
        };
        let validate_manual = self.voice_settings.tts_picker_minimax_provenance()
            == tts::VoiceSelectionProvenance::Manual;
        self.preview_tts_selection_work(
            move |runtime, cancelled| {
                super::super::tts_catalog::synthesize_tts_preview(&runtime, &selected, cancelled)
            },
            validate_manual,
            true,
            cx,
        );
    }

    fn select_minimax_tts_model(&mut self, model_id: &str, cx: &mut Context<Self>) {
        let model_id =
            tts::TtsModelId::parse(model_id).expect("curated MiniMax model ID must be valid");
        if self.voice_settings.tts_picker_provider_id() == Some(tts::MINIMAX_TTS_PROVIDER_ID) {
            self.voice_settings.set_tts_picker_minimax_model(model_id);
            cx.notify();
            return;
        }
        let changed = self.voice_settings.set_minimax_tts_model(model_id);
        if changed {
            self.configure_active_minimax_tts_if_ready(cx);
        }
        self.persist_voice_settings_in_background(cx);
        cx.notify();
    }

    fn start_minimax_tts_validation(
        &mut self,
        preview_after_validation: bool,
        cx: &mut Context<Self>,
    ) {
        let (generation, api_key) = match self.voice_settings.begin_minimax_connection() {
            Ok(connection) => connection,
            Err(error) => {
                if preview_after_validation {
                    self.pending_minimax_preview_after_validation = None;
                }
                self.voice_settings.report_tts_status(error, true);
                cx.notify();
                return;
            }
        };
        if preview_after_validation {
            self.pending_minimax_preview_after_validation = Some(generation);
        }
        let api_key_for_request = api_key.clone();
        let work = cx
            .background_executor()
            .spawn(async move { query_minimax_voice_catalog(api_key_for_request) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok(catalog) => {
                        let accepted = app
                            .voice_settings
                            .record_minimax_connection_success(generation, api_key, catalog);
                        if accepted {
                            if app.voice_settings.tts_picker_provider_id()
                                == Some(tts::MINIMAX_TTS_PROVIDER_ID)
                                && app.voice_settings.tts_picker_minimax_model_id().is_none()
                            {
                                app.voice_settings
                                    .set_tts_picker_minimax_model(minimax_turbo_model_id());
                            } else if app
                                .voice_settings
                                .values()
                                .minimax_tts_model_id()
                                .is_none()
                            {
                                app.voice_settings
                                    .set_minimax_tts_model(minimax_turbo_model_id());
                            }
                            let credential_write_pending =
                                app.voice_settings.credential_persistence_pending();
                            app.persist_voice_settings_in_background(cx);
                            if preview_after_validation && !credential_write_pending {
                                app.pending_minimax_preview_after_validation = None;
                                app.start_minimax_tts_preview(cx);
                            } else if !credential_write_pending {
                                app.configure_active_minimax_tts_if_ready(cx);
                            }
                        } else if app.pending_minimax_preview_after_validation == Some(generation) {
                            app.pending_minimax_preview_after_validation = None;
                        }
                    }
                    Err(error) => {
                        let disposition = app.voice_settings.record_minimax_connection_failure(
                            generation,
                            error.kind(),
                            format!("MiniMax connection failed: {error}"),
                        );
                        if disposition
                            == crate::settings::voice::state::MiniMaxConnectionFailureDisposition::Stale
                        {
                            if app.pending_minimax_preview_after_validation == Some(generation) {
                                app.pending_minimax_preview_after_validation = None;
                            }
                            return;
                        }
                        if preview_after_validation {
                            app.pending_minimax_preview_after_validation = None;
                        }
                        let active_credential_failed = disposition
                            == crate::settings::voice::state::MiniMaxConnectionFailureDisposition::ActiveCredentialFailed;
                        let minimax_active = active_credential_failed && app
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
        if !active {
            return;
        }
        let selected = (self.voice_settings.minimax_readiness() == tts::RemoteTtsReadiness::Ready)
            .then(|| selected_minimax_tts(&self.voice_settings))
            .flatten();
        self.invalidate_tts_for_configuration_change(cx);
        crate::app::dialogue::configure_selected_tts_model(selected);
    }
}
