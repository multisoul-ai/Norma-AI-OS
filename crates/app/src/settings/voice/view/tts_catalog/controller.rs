use crate::app::MultisoulApp;
use crate::settings::voice::TtsModelInstallState;
use gpui::Context;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

mod work;
use work::{install_tts_package, synthesize_tts_preview, validate_tts_selection};

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
            (manager, configured)
        });
        cx.spawn(async move |this, cx| {
            let (manager, configured) = work.await;
            let _ = this.update(cx, |app, cx| {
                if scan_generation != app.tts_scan_generation {
                    return;
                }
                app.voice_settings.replace_tts_model_manager(manager);
                match configured {
                    Ok(Some(model)) => crate::app::dialogue::configure_selected_tts_model(Some(model)),
                    Ok(None) => {
                        crate::app::dialogue::configure_selected_tts_model(None);
                        app.voice_settings.report_tts_status(
                            "No Local TTS model is selected; replies will stay text-only.",
                            false,
                        );
                    }
                    Err(error) => {
                        crate::app::dialogue::configure_selected_tts_model(None);
                        app.voice_settings.report_tts_status(
                            format!(
                                "Saved Local TTS selection is unavailable; replies will stay text-only: {error}"
                            ),
                            true,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn install_tts_model(&mut self, id: tts::TtsModelId, cx: &mut Context<Self>) {
        if self.tts_download_cancellations.contains_key(&id) {
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
                        let _ = tx.try_send(TtsInstallEvent::Progress(downloaded, total));
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
                                    if !app.has_valid_selected_tts_model() {
                                        app.select_tts_model(id.clone(), None, cx);
                                    }
                                }
                                TtsInstallOutcome::Cancelled => {
                                    app.voice_settings
                                        .tts_model_manager_mut()
                                        .mark_not_installed(&id);
                                    app.voice_settings
                                        .report_tts_status("Local TTS download cancelled.", false);
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
                "Select another Local TTS model or disable Local TTS before deleting this model.",
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
        self.cancel_agent_announcements();
        self.invalidate_agent_announcement_tts_client(cx);
        self.tts_preview.cancel();
        self.dialogue.cancel_tts_for_configuration_change();
        self.tts_selection_generation = self.tts_selection_generation.wrapping_add(1);
        self.invalidate_tts_scan();
        let selection_generation = self.tts_selection_generation;
        let root = self.voice_settings.tts_model_root();
        let work = cx
            .background_executor()
            .spawn(async move { validate_tts_selection(&root, model_id, requested_voice) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                if selection_generation != app.tts_selection_generation {
                    return;
                }
                match result {
                    Ok(selected) => app.commit_tts_selection(selected, cx),
                    Err(error) => app.voice_settings.report_tts_status(
                        format!("Could not select Local TTS model: {error}"),
                        true,
                    ),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn preview_tts_voice(
        &mut self,
        model_id: tts::TtsModelId,
        voice_id: tts::TtsVoiceId,
        cx: &mut Context<Self>,
    ) {
        if self.dialogue.phase() == dialogue::DialoguePhase::Speaking {
            self.voice_settings.report_tts_status(
                "Voice preview is unavailable while an assistant reply is playing.",
                false,
            );
            cx.notify();
            return;
        }
        self.cancel_agent_announcements();
        let root = self.voice_settings.tts_model_root();
        let configuration_generation = self.dialogue.tts_configuration_generation();
        let (preview_generation, control) = self.tts_preview.start();
        self.dialogue.track_tts_preview(Arc::clone(&control));
        let work = cx.background_executor().spawn(async move {
            let selected = validate_tts_selection(&root, model_id, Some(voice_id))?;
            synthesize_tts_preview(&selected)
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                let preview_is_current = app.tts_preview.is_current(preview_generation);
                let same_configuration =
                    app.dialogue.tts_configuration_generation() == configuration_generation;
                match result {
                    Ok(chunk)
                        if preview_is_current
                            && same_configuration
                            && app.dialogue.phase() != dialogue::DialoguePhase::Speaking =>
                    {
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
                                            "Voice preview playback failed.",
                                            true,
                                        );
                                        cx.notify();
                                    }
                                });
                                return;
                            };
                            if this
                                .update(cx, |app, cx| {
                                    app.tts_preview.queue_pcm(preview_generation, &chunk);
                                    cx.notify();
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
                        app.voice_settings
                            .report_tts_status(format!("Voice preview failed: {error}"), true);
                    }
                    Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn has_valid_selected_tts_model(&self) -> bool {
        let Some(id) = self.voice_settings.values().tts_model_id() else {
            return false;
        };
        matches!(
            self.voice_settings.tts_model_manager().installation(id),
            Some(TtsModelInstallState::Ready)
        )
    }

    fn commit_tts_selection(
        &mut self,
        selected: crate::app::dialogue::SelectedTtsModel,
        cx: &mut Context<Self>,
    ) {
        self.stop_continuous_listening(cx);
        self.voice.stop_for_model_switch();
        self.voice_settings.select_tts_model_and_voice(
            Some(selected.model_id.clone()),
            Some(selected.voice_id.clone()),
        );
        crate::app::dialogue::configure_selected_tts_model(Some(selected));
        self.persist_voice_settings_in_background(cx);
    }

    fn invalidate_tts_scan(&mut self) {
        self.tts_scan_generation = self.tts_scan_generation.wrapping_add(1);
    }

    fn remove_cancelled_tts_install(&mut self, id: tts::TtsModelId, cx: &mut Context<Self>) {
        let catalog = tts::builtin_model_catalog();
        let Some(descriptor) = catalog.find(&id).cloned() else {
            return;
        };
        let root = self.voice_settings.tts_model_root().join(id.as_str());
        let work = cx.background_executor().spawn(async move {
            tts::remove_installation(&root, &descriptor, None).map_err(|error| error.to_string())
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
                }
                cx.notify();
            });
        })
        .detach();
    }
}
