use super::*;

impl MultisoulApp {
    pub(super) fn select_tts_model_work(
        &mut self,
        model_id: tts::TtsModelId,
        requested_voice: Option<tts::TtsVoiceId>,
        picker_apply: bool,
        cx: &mut Context<Self>,
    ) {
        let picker_selecting_local = self.voice_settings.tts_picker_provider_id()
            == Some(tts::LOCAL_TTS_PROVIDER_ID)
            && !picker_apply;
        if picker_selecting_local {
            let catalog = tts::builtin_model_catalog();
            let Some(descriptor) = catalog.find(&model_id) else {
                return;
            };
            let voice_id = descriptor
                .resolve_voice_id(requested_voice.as_ref().map(tts::TtsVoiceId::as_str))
                .clone();
            self.tts_preview.cancel();
            self.voice_settings
                .set_tts_picker_local_selection(model_id, voice_id);
            cx.notify();
            return;
        }
        self.cancel_agent_announcements();
        self.tts_preview.cancel();
        self.invalidate_tts_for_configuration_change(cx);
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
                if !app.voice_settings.tts_picker_apply_pending()
                    || app.voice_settings.tts_picker_provider_id()
                        != Some(tts::LOCAL_TTS_PROVIDER_ID)
                {
                    return;
                }
                match result {
                    Ok(selected) => app.commit_tts_selection(selected, cx),
                    Err(error) => app.voice_settings.fail_tts_picker_apply(ui::i18n::message(
                        "voice.could_not_select_local_tts_model_error",
                        &[("error", (error).to_string())],
                    )),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
