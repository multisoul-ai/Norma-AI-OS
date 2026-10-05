mod selection;
use crate::app::MultisoulApp;
use crate::settings::voice::TtsModelInstallState;
use gpui::Context;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

mod maintenance;
mod reference;
mod work;
pub(crate) use work::synthesize_tts_preview;
use work::{install_tts_package, resolve_tts_selection, validate_tts_selection};

#[cfg(test)]
#[path = "../../../../../tests/unit/voice_settings_tts_scan_tests.rs"]
mod scan_status_tests;

enum TtsInstallEvent {
    Progress(u64, u64),
    Finished(TtsInstallOutcome),
}

enum TtsInstallOutcome {
    Ready,
    Cancelled,
    Failed(String),
}

impl MultisoulApp {
    pub(crate) fn refresh_tts_model_states(&mut self, cx: &mut Context<Self>) {
        self.invalidate_tts_scan();
        let scan_generation = self.tts_scan_generation;
        let root = self.voice_settings.tts_model_root();
        let selected_model = self.voice_settings.values().tts_model_id().cloned();
        let selected_voice = self.voice_settings.values().tts_voice_id().cloned();
        let work = cx.background_executor().spawn(async move {
            let manager = crate::settings::voice::TtsModelManagerState::scan(
                &tts::builtin_model_catalog(),
                &root,
            );
            let configured = selected_model
                .map(|model_id| validate_tts_selection(&root, model_id, selected_voice))
                .transpose();
            let reference = tts::load_zipvoice_reference(&root);
            (manager, configured, reference)
        });
        cx.spawn(async move |this, cx| {
            let (manager, configured, reference) = work.await;
            let _ = this.update(cx, |app, cx| {
                if scan_generation != app.tts_scan_generation {
                    return;
                }
                app.voice_settings.replace_tts_model_manager(manager);
                let local_active = app
                    .voice_settings
                    .values()
                    .active_tts_provider_id()
                    .is_some_and(|provider| provider.as_str() == tts::LOCAL_TTS_PROVIDER_ID);
                match configured {
                    Ok(Some(model)) if local_active => {
                        app.dialogue.selected_tts_model = Some(model)
                    }
                    Ok(Some(_)) => {}
                    Ok(None) if local_active => {
                        app.dialogue.selected_tts_model = None;
                        app.voice_settings.report_tts_status(
                            ui::i18n::LocalizedText::new("voice.no_local_tts_model_is_selected_replies_will_stay_text_only"),
                            false,
                        );
                    }
                    Err(error) if local_active => {
                        app.dialogue.selected_tts_model = None;
                        app.voice_settings.report_tts_status(
                            ui::i18n::message("voice.saved_local_tts_selection_is_unavailable_replies_will_stay_text_only", &[("error", (error).to_string())]),
                            true,
                        );
                    }
                    Ok(None) | Err(_) => {}
                }
                app.voice_settings.apply_zipvoice_reference_scan(reference);
                app.start_next_agent_announcement(cx);
                app.complete_voice_setup_if_ready(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn install_tts_model(&mut self, id: tts::TtsModelId, cx: &mut Context<Self>) {
        if self.tts_download_cancellations.contains_key(&id) {
            return;
        }
        if matches!(
            self.voice_settings.tts_model_manager().installation(&id),
            Some(TtsModelInstallState::Ready)
                | Some(TtsModelInstallState::Installing { .. })
                | Some(TtsModelInstallState::Cancelling { .. })
        ) {
            return;
        }
        let catalog = tts::builtin_model_catalog();
        let Some(descriptor) = catalog.find(&id).cloned() else {
            return;
        };
        let root = self.voice_settings.tts_model_root();
        self.invalidate_tts_scan();
        let cancelled = Arc::new(AtomicBool::new(false));
        self.tts_download_cancellations
            .insert(id.clone(), Arc::clone(&cancelled));
        self.voice_settings
            .tts_model_manager_mut()
            .begin_install(&id, descriptor.package().download_bytes());
        let (tx, rx) = async_channel::bounded(32);
        cx.background_executor()
            .spawn(async move {
                let outcome =
                    install_tts_package(&descriptor, &root, &cancelled, |downloaded, total| {
                        let _ = tx.send_blocking(TtsInstallEvent::Progress(downloaded, total));
                    })
                    .await;
                let _ = tx.send_blocking(TtsInstallEvent::Finished(outcome));
            })
            .detach();
        cx.spawn(async move |this, cx| {
            while let Ok(event) = rx.recv().await {
                let terminal = matches!(event, TtsInstallEvent::Finished(_));
                let _ = this.update(cx, |app, cx| {
                    match event {
                        TtsInstallEvent::Progress(downloaded, total) => {
                            app.invalidate_tts_scan();
                            app.voice_settings
                                .tts_model_manager_mut()
                                .update_progress(&id, downloaded, total);
                        }
                        TtsInstallEvent::Finished(outcome) => {
                            let cancelled = app
                                .tts_download_cancellations
                                .get(&id)
                                .is_some_and(|flag| flag.load(Ordering::Acquire));
                            app.tts_download_cancellations.remove(&id);
                            app.invalidate_tts_scan();
                            match outcome {
                                TtsInstallOutcome::Ready if cancelled => {
                                    app.remove_cancelled_tts_install(id.clone(), cx);
                                }
                                TtsInstallOutcome::Ready => {
                                    app.voice_settings.tts_model_manager_mut().mark_ready(&id);
                                    let picker_needs_local_selection =
                                        app.voice_settings.tts_picker_provider_id()
                                            == Some(tts::LOCAL_TTS_PROVIDER_ID)
                                            && !app.voice_settings.tts_picker_ready();
                                    if picker_needs_local_selection {
                                        app.select_tts_model(id.clone(), None, cx);
                                    }
                                }
                                TtsInstallOutcome::Cancelled => {
                                    app.voice_settings
                                        .tts_model_manager_mut()
                                        .mark_not_installed(&id);
                                    app.voice_settings.report_tts_status(
                                        ui::i18n::LocalizedText::new(
                                            "voice.local_tts_download_cancelled",
                                        ),
                                        false,
                                    );
                                }
                                TtsInstallOutcome::Failed(error) => app
                                    .voice_settings
                                    .tts_model_manager_mut()
                                    .mark_failed(&id, error),
                            }
                        }
                    }
                    cx.notify();
                });
                if terminal {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn cancel_tts_install(&mut self, id: tts::TtsModelId, cx: &mut Context<Self>) {
        let Some(cancelled) = self.tts_download_cancellations.get(&id) else {
            return;
        };
        cancelled.store(true, Ordering::Release);
        #[cfg(feature = "e2e-test-support")]
        crate::settings::voice::tts_models::release_e2e_fake_tts_transport();
        self.invalidate_tts_scan();
        self.voice_settings
            .tts_model_manager_mut()
            .begin_cancellation(&id);
        cx.notify();
    }

    pub(crate) fn delete_tts_model(&mut self, id: tts::TtsModelId, cx: &mut Context<Self>) {
        if self.voice_settings.values().tts_model_id() == Some(&id) {
            self.voice_settings.report_tts_status(
                ui::i18n::LocalizedText::new(
                    "voice.select_another_local_tts_model_or_disable_local_tts_before_deleting",
                ),
                true,
            );
            cx.notify();
            return;
        }
        let catalog = tts::builtin_model_catalog();
        let Some(descriptor) = catalog.find(&id).cloned() else {
            return;
        };
        let root = self.voice_settings.tts_model_root().join(id.as_str());
        let selected = self.voice_settings.values().tts_model_id().cloned();
        self.invalidate_tts_scan();
        #[cfg(feature = "e2e-test-support")]
        let fake_transport = crate::settings::voice::tts_models::e2e_fake_tts_transport_enabled();
        #[cfg(not(feature = "e2e-test-support"))]
        let fake_transport = false;
        let work = cx.background_executor().spawn(async move {
            if fake_transport {
                Ok(())
            } else {
                tts::remove_installation(&root, &descriptor, selected.as_ref())
                    .map_err(|error| error.to_string())
            }
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                app.invalidate_tts_scan();
                match result {
                    Ok(()) => app
                        .voice_settings
                        .tts_model_manager_mut()
                        .mark_not_installed(&id),
                    Err(error) => app.voice_settings.report_tts_status(error, true),
                };
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn select_tts_model(
        &mut self,
        model_id: tts::TtsModelId,
        requested_voice: Option<tts::TtsVoiceId>,
        cx: &mut Context<Self>,
    ) {
        if self.voice_settings.tts_picker_apply_pending() {
            self.voice_settings.report_tts_status(
                ui::i18n::LocalizedText::new(
                    "voice.wait_for_the_selected_voice_to_finish_applying_before_changing_the",
                ),
                false,
            );
            cx.notify();
            return;
        }
        self.select_tts_model_work(model_id, requested_voice, false, cx);
    }

    pub(crate) fn apply_tts_picker_model(
        &mut self,
        model_id: tts::TtsModelId,
        voice_id: tts::TtsVoiceId,
        cx: &mut Context<Self>,
    ) {
        self.select_tts_model_work(model_id, Some(voice_id), true, cx);
    }

    pub(crate) fn preview_tts_voice(
        &mut self,
        model_id: tts::TtsModelId,
        voice_id: tts::TtsVoiceId,
        cx: &mut Context<Self>,
    ) {
        let root = self.voice_settings.tts_model_root();
        self.preview_tts_selection_work(
            move |tts_runtime, cancelled| {
                let selected = resolve_tts_selection(&root, model_id, Some(voice_id))?;
                synthesize_tts_preview(&tts_runtime, &selected, cancelled)
            },
            false,
            false,
            cx,
        );
    }

    pub(crate) fn preview_tts_selection_work(
        &mut self,
        synthesis: impl FnOnce(
                crate::app::dialogue::SharedTtsRuntime,
                Arc<AtomicBool>,
            ) -> Result<audio::PcmAudioChunk, String>
            + Send
            + 'static,
        validate_manual_minimax_voice: bool,
        minimax_preview: bool,
        cx: &mut Context<Self>,
    ) {
        if self.dialogue.phase() == dialogue::DialoguePhase::Speaking {
            self.voice_settings.report_tts_status(
                ui::i18n::LocalizedText::new(
                    "voice.voice_preview_is_unavailable_while_an_assistant_reply_is_playing",
                ),
                false,
            );
            cx.notify();
            return;
        }
        self.cancel_agent_announcements();
        let configuration_generation = self.dialogue.tts_configuration_generation();
        let minimax_profile_generation =
            minimax_preview.then(|| self.voice_settings.minimax_profile_generation());
        let (preview_generation, control) = self.tts_preview.start();
        if minimax_preview {
            self.voice_settings
                .begin_minimax_preview(preview_generation);
        }
        self.dialogue.track_tts_preview(Arc::clone(&control));
        cx.notify();
        let tts_runtime = self.dialogue.shared_tts_runtime();
        let cancelled = control.cancellation_flag();
        let work = cx
            .background_executor()
            .spawn(async move { synthesis(tts_runtime, cancelled) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                let preview_is_current = app.tts_preview.is_current(preview_generation);
                let same_configuration =
                    app.dialogue.tts_configuration_generation() == configuration_generation;
                let same_minimax_profile = minimax_profile_generation.is_none_or(|generation| {
                    app.voice_settings.minimax_profile_generation() == generation
                });
                if minimax_preview {
                    app.voice_settings
                        .finish_minimax_preview(preview_generation);
                }
                match result {
                    Ok(chunk)
                        if preview_is_current
                            && same_configuration
                            && same_minimax_profile
                            && app.dialogue.phase() != dialogue::DialoguePhase::Speaking =>
                    {
                        if validate_manual_minimax_voice {
                            let validation_recorded =
                                app.voice_settings.mark_minimax_manual_voice_validated(
                                    minimax_profile_generation.expect(
                                        "manual MiniMax validation requires profile generation",
                                    ),
                                );
                            app.persist_voice_settings_in_background(cx);
                            if validation_recorded
                                && !app.voice_settings.settings_persistence_pending()
                            {
                                app.configure_active_minimax_tts_if_ready(cx);
                            }
                        }
                        let control = Arc::clone(&control);
                        let playback_work = cx.background_executor().spawn(async move {
                            let result = control.play_with_completion(&chunk);
                            (chunk, result)
                        });
                        cx.spawn(async move |this, cx| {
                            let (chunk, playback) = playback_work.await;
                            let Ok((started_rx, finished_rx)) = playback else {
                                let _ = this.update(cx, |app, cx| {
                                    if app.tts_preview.is_current(preview_generation) {
                                        app.tts_preview.cancel();
                                        app.voice_settings.report_tts_status(
                                            ui::i18n::LocalizedText::new(
                                                "voice.voice_preview_playback_failed",
                                            ),
                                            true,
                                        );
                                        cx.notify();
                                    }
                                });
                                return;
                            };
                            if this
                                .update(cx, |app, _cx| {
                                    app.tts_preview.queue_pcm(preview_generation, &chunk);
                                })
                                .is_err()
                            {
                                return;
                            }
                            if started_rx.recv().await.is_ok()
                                && this
                                    .update(cx, |app, cx| {
                                        app.tts_preview.mark_playback_started(preview_generation);
                                        cx.notify();
                                    })
                                    .is_err()
                            {
                                return;
                            }
                            let completed = finished_rx.recv().await.is_ok();
                            let _ = this.update(cx, |app, cx| {
                                if completed {
                                    app.tts_preview.finish_playback(preview_generation);
                                } else if app.tts_preview.is_current(preview_generation) {
                                    app.tts_preview.cancel();
                                }
                                app.start_next_agent_announcement(cx);
                                cx.notify();
                            });
                        })
                        .detach();
                    }
                    Ok(_) if preview_is_current => {
                        app.tts_preview.cancel();
                        app.start_next_agent_announcement(cx);
                    }
                    Ok(_) => {}
                    Err(error) if preview_is_current => {
                        app.tts_preview.cancel();
                        app.start_next_agent_announcement(cx);
                        app.voice_settings.report_tts_status(
                            ui::i18n::message(
                                "voice.voice_preview_failed_error",
                                &[("error", (error).to_string())],
                            ),
                            true,
                        );
                    }
                    Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }
}
