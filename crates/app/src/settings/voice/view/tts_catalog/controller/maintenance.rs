use crate::app::MultisoulApp;
use gpui::Context;

impl MultisoulApp {
    pub(super) fn commit_tts_selection(
        &mut self,
        selected: crate::app::dialogue::SelectedTtsModel,
        cx: &mut Context<Self>,
    ) {
        let close_picker = self.voice_settings.tts_picker_apply_pending();
        if close_picker {
            if self.voice_settings.settings_persistence_pending() {
                self.voice_settings
                    .fail_tts_picker_apply(ui::i18n::LocalizedText::new(
                        "voice.wait_for_the_current_settings_save_to_finish_before_applying_this",
                    ));
                cx.notify();
                return;
            }
            let rollback = self.voice_settings.capture_tts_activation_rollback();
            self.voice_settings.select_tts_model_and_voice(
                Some(selected.model_id.clone()),
                Some(selected.voice_id.clone()),
            );
            self.voice_settings.select_tts_provider(Some(
                tts::TtsProviderId::parse(tts::LOCAL_TTS_PROVIDER_ID)
                    .expect("built-in On-device TTS Provider ID must be valid"),
            ));
            self.pending_tts_activation = Some(crate::app::dialogue::PendingTtsActivation {
                selected,
                rollback,
                close_picker_on_success: true,
            });
            if !self.voice_settings.settings_persistence_pending() {
                let activation = self
                    .pending_tts_activation
                    .take()
                    .expect("session-only On-device activation must remain pending until applied");
                self.complete_tts_activation(activation, cx);
            } else {
                self.persist_voice_settings_in_background(cx);
            }
            cx.notify();
            return;
        }
        self.stop_continuous_listening(cx);
        self.voice.stop_for_model_switch();
        self.voice_settings.select_tts_model_and_voice(
            Some(selected.model_id.clone()),
            Some(selected.voice_id.clone()),
        );
        self.voice_settings.select_tts_provider(Some(
            tts::TtsProviderId::parse(tts::LOCAL_TTS_PROVIDER_ID)
                .expect("built-in On-device TTS Provider ID must be valid"),
        ));
        self.dialogue.selected_tts_model = Some(selected);
        self.start_next_agent_announcement(cx);
        self.complete_voice_setup_if_ready(cx);
        self.persist_voice_settings_in_background(cx);
    }

    pub(super) fn invalidate_tts_scan(&mut self) {
        self.tts_scan_generation = self.tts_scan_generation.wrapping_add(1);
    }

    pub(super) fn remove_cancelled_tts_install(
        &mut self,
        id: tts::TtsModelId,
        cx: &mut Context<Self>,
    ) {
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
