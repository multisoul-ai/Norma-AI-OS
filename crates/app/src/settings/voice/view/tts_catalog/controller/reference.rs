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
                return Err(
                    ui::i18n::text("voice.choose_a_local_wav_file_or_enter_its_path").to_string(),
                );
            }
            tts::import_zipvoice_reference(&PathBuf::from(path.trim()), &transcript, &model_root)
                .map_err(|error| error.to_string())
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                app.voice_settings
                    .finish_zipvoice_reference_import(generation, result);
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
                app.voice_settings
                    .finish_zipvoice_reference_remove(generation, result);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
