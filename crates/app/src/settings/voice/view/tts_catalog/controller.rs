use crate::app::MultisoulApp;
use crate::settings::voice::TtsModelInstallState;
use gpui::Context;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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
                        cx.background_executor()
                            .spawn(async move { control.play(&chunk) })
                            .detach();
                    }
                    Ok(_) => {}
                    Err(error) if preview_is_current => app
                        .voice_settings
                        .report_tts_status(format!("Voice preview failed: {error}"), true),
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

fn synthesize_tts_preview(
    selected: &crate::app::dialogue::SelectedTtsModel,
) -> Result<audio::PcmAudioChunk, String> {
    #[cfg(feature = "e2e-test-support")]
    if crate::settings::voice::tts_models::e2e_fake_tts_transport_enabled() {
        return audio::PcmAudioChunk::new(audio::PcmFormat::Pcm16, 16_000, 1, vec![0; 320])
            .map_err(|error| format!("invalid fake preview PCM: {error:?}"));
    }
    let catalog = tts::builtin_model_catalog();
    let descriptor = catalog
        .find(&selected.model_id)
        .ok_or_else(|| format!("unknown TTS model {}", selected.model_id.as_str()))?;
    let available =
        tts::validate_installation(&selected.root.join(selected.model_id.as_str()), descriptor)
            .map_err(|error| error.to_string())?;
    let mut client =
        voice_sherpa::SherpaTtsClient::from_available_model(&available, &selected.voice_id)
            .map_err(|error| error.to_string())?;
    dialogue::TtsClient::synthesize(&mut client, "你好，这是当前语音的试听。")
        .map_err(|error| error.to_string())
}

fn validate_tts_selection(
    root: &std::path::Path,
    model_id: tts::TtsModelId,
    requested_voice: Option<tts::TtsVoiceId>,
) -> Result<crate::app::dialogue::SelectedTtsModel, String> {
    let catalog = tts::builtin_model_catalog();
    let descriptor = catalog
        .find(&model_id)
        .ok_or_else(|| format!("unknown TTS model {}", model_id.as_str()))?;
    let voice_id = descriptor
        .resolve_voice_id(requested_voice.as_ref().map(tts::TtsVoiceId::as_str))
        .clone();
    #[cfg(feature = "e2e-test-support")]
    if crate::settings::voice::tts_models::e2e_fake_tts_transport_enabled() {
        return Ok(crate::app::dialogue::SelectedTtsModel {
            root: root.to_path_buf(),
            model_id,
            voice_id,
        });
    }
    let available = tts::validate_installation(&root.join(model_id.as_str()), descriptor)
        .map_err(|error| error.to_string())?;
    voice_sherpa::SherpaTtsClient::from_available_model(&available, &voice_id)
        .map_err(|error| error.to_string())?;
    Ok(crate::app::dialogue::SelectedTtsModel {
        root: root.to_path_buf(),
        model_id,
        voice_id,
    })
}

async fn install_tts_package(
    descriptor: &tts::TtsModelDescriptor,
    root: &std::path::Path,
    cancelled: &AtomicBool,
    progress: impl FnMut(u64, u64),
) -> TtsInstallOutcome {
    #[cfg(feature = "e2e-test-support")]
    if crate::settings::voice::tts_models::e2e_fake_tts_transport_enabled() {
        let mut progress = progress;
        progress(0, 1);
        if crate::settings::voice::tts_models::e2e_fake_tts_transport_is_held() {
            if let Some(release) =
                crate::settings::voice::tts_models::take_e2e_fake_tts_release_receiver()
            {
                let _ = release.recv().await;
            }
        }
        if cancelled.load(Ordering::Acquire) {
            return TtsInstallOutcome::Cancelled;
        }
        progress(1, 1);
        return TtsInstallOutcome::Ready;
    }
    let files = match crate::settings::voice::tts_models::download_tts_package(
        descriptor,
        &root.join(".downloads"),
        cancelled,
        progress,
    ) {
        Ok(files) => files,
        Err(_) if cancelled.load(Ordering::Acquire) => return TtsInstallOutcome::Cancelled,
        Err(error) => return TtsInstallOutcome::Failed(error),
    };
    if cancelled.load(Ordering::Acquire) {
        cleanup_downloaded_package(&files);
        return TtsInstallOutcome::Cancelled;
    }
    let installation =
        tts::install_downloaded_package(descriptor, &files.archive, &files.support, root);
    cleanup_downloaded_package(&files);
    if cancelled.load(Ordering::Acquire) {
        let _ = tts::remove_installation(&root.join(descriptor.id().as_str()), descriptor, None);
        return TtsInstallOutcome::Cancelled;
    }
    installation
        .map(|_| TtsInstallOutcome::Ready)
        .unwrap_or_else(|error| TtsInstallOutcome::Failed(error.to_string()))
}

fn cleanup_downloaded_package(files: &crate::settings::voice::tts_models::DownloadedTtsPackage) {
    let _ = std::fs::remove_file(&files.archive);
    for file in &files.support {
        let _ = std::fs::remove_file(file);
    }
}
