use crate::app::file_picker::pick_zipvoice_reference_audio;
use crate::app::MultisoulApp;
use gpui::Context;
use std::path::PathBuf;

impl MultisoulApp {
    pub(crate) fn choose_zipvoice_reference_audio(&mut self, cx: &mut Context<Self>) {
        if self.voice_settings.zipvoice_reference_operation_busy() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let Some(path) = pick_zipvoice_reference_audio().await else {
                return;
            };
            let _ = this.update(cx, |app, cx| {
                app.voice_settings
                    .set_zipvoice_reference_draft_path(path.to_string_lossy().into_owned(), true);
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn import_zipvoice_reference(&mut self, cx: &mut Context<Self>) {
        let Some((generation, path, transcript)) =
            self.voice_settings.begin_zipvoice_reference_import()
        else {
            return;
        };
        let model_root = self.voice_settings.tts_model_root();
        let work = cx.background_executor().spawn(async move {
            if path.trim().is_empty() {
                return Err("choose a local WAV file or enter its path".to_string());
            }
            tts::import_zipvoice_reference(&PathBuf::from(path.trim()), &transcript, &model_root)
                .map_err(|error| error.to_string())
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                let imported = app
                    .voice_settings
                    .finish_zipvoice_reference_import(generation, result);
                if imported
                    && app
                        .voice_settings
                        .values()
                        .tts_model_id()
                        .is_some_and(|id| id.as_str() == tts::ZIPVOICE_DISTILL_INT8_MODEL_ID)
                {
                    app.cancel_agent_announcements();
                    app.invalidate_agent_announcement_tts_client(cx);
                    app.tts_preview.cancel();
                    app.dialogue.cancel_tts_for_configuration_change();
                    app.stop_continuous_listening(cx);
                    app.voice.stop_for_model_switch();
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn remove_zipvoice_reference(&mut self, cx: &mut Context<Self>) {
        let Some(generation) = self.voice_settings.begin_zipvoice_reference_remove() else {
            return;
        };
        let model_root = self.voice_settings.tts_model_root();
        let work = cx.background_executor().spawn(async move {
            tts::remove_zipvoice_reference(&model_root).map_err(|error| error.to_string())
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                let removed = app
                    .voice_settings
                    .finish_zipvoice_reference_remove(generation, result);
                if removed
                    && app
                        .voice_settings
                        .values()
                        .tts_model_id()
                        .is_some_and(|id| id.as_str() == tts::ZIPVOICE_DISTILL_INT8_MODEL_ID)
                {
                    app.cancel_agent_announcements();
                    app.invalidate_agent_announcement_tts_client(cx);
                    app.tts_preview.cancel();
                    app.dialogue.cancel_tts_for_configuration_change();
                    app.stop_continuous_listening(cx);
                    app.voice.stop_for_model_switch();
                    app.voice_settings.select_tts_model_and_voice(None, None);
                    crate::app::dialogue::configure_selected_tts_model(None);
                    app.persist_voice_settings_in_background(cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
