use crate::app::MultisoulApp;
use gpui::{ClickEvent, Context, Window};

use super::work::{minimax_provider_id, selected_minimax_tts};

impl MultisoulApp {
    pub(crate) fn on_voice_apply_tts_picker(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.voice_settings.tts_picker_provider_id() {
            Some(tts::MINIMAX_TTS_PROVIDER_ID) => self.apply_minimax_tts_picker(cx),
            Some(tts::LOCAL_TTS_PROVIDER_ID) => self.apply_local_tts_picker(cx),
            _ => {
                self.voice_settings
                    .report_tts_status("Choose a TTS Provider before applying a voice.", true);
                cx.notify();
            }
        }
    }

    fn apply_minimax_tts_picker(&mut self, cx: &mut Context<Self>) {
        if self.voice_settings.settings_persistence_pending() {
            self.report_tts_activation_save_wait(cx);
            return;
        }
        if self.voice_settings.tts_picker_minimax_readiness() != tts::RemoteTtsReadiness::Ready {
            self.voice_settings.report_tts_status(
                "Connect MiniMax and choose a verified Voice before applying.",
                true,
            );
            cx.notify();
            return;
        }
        let rollback = self.voice_settings.capture_tts_activation_rollback();
        self.voice_settings.begin_tts_picker_apply();
        if !self.voice_settings.commit_tts_picker_minimax_profile() {
            self.voice_settings.fail_tts_picker_apply(
                "Connect MiniMax and choose a verified Voice before applying.",
            );
            cx.notify();
            return;
        }
        self.activate_minimax_tts(rollback, cx);
    }

    fn apply_local_tts_picker(&mut self, cx: &mut Context<Self>) {
        if self.voice_settings.settings_persistence_pending() {
            self.report_tts_activation_save_wait(cx);
            return;
        }
        let (Some(model_id), Some(voice_id)) = (
            self.voice_settings.tts_picker_local_model_id().cloned(),
            self.voice_settings.tts_picker_local_voice_id().cloned(),
        ) else {
            self.voice_settings
                .report_tts_status("Choose a ready On-device Voice before applying.", true);
            cx.notify();
            return;
        };
        self.voice_settings.begin_tts_picker_apply();
        self.apply_tts_picker_model(model_id, voice_id, cx);
    }

    fn report_tts_activation_save_wait(&mut self, cx: &mut Context<Self>) {
        self.voice_settings.report_tts_status(
            "Wait for the current settings save to finish before applying this Voice.",
            false,
        );
        cx.notify();
    }

    fn activate_minimax_tts(
        &mut self,
        rollback: crate::settings::voice::TtsActivationRollback,
        cx: &mut Context<Self>,
    ) {
        let Some(selected) = selected_minimax_tts(&self.voice_settings) else {
            self.voice_settings.restore_tts_activation(rollback);
            self.voice_settings
                .fail_tts_picker_apply("MiniMax configuration is incomplete.");
            cx.notify();
            return;
        };
        self.voice_settings
            .select_tts_provider(Some(minimax_provider_id()));
        self.pending_tts_activation = Some(crate::app::dialogue::PendingTtsActivation {
            selected,
            rollback,
            close_picker_on_success: true,
        });
        if !self.voice_settings.settings_persistence_pending() {
            let activation = self
                .pending_tts_activation
                .take()
                .expect("session-only MiniMax activation must remain pending until applied");
            self.complete_tts_activation(activation, cx);
            cx.notify();
            return;
        }
        self.persist_voice_settings_in_background(cx);
        cx.notify();
    }

    pub(crate) fn complete_tts_activation(
        &mut self,
        activation: crate::app::dialogue::PendingTtsActivation,
        cx: &mut Context<Self>,
    ) {
        self.cancel_agent_announcements();
        self.tts_preview.cancel();
        self.stop_continuous_listening(cx);
        self.voice.stop_for_model_switch();
        self.invalidate_tts_for_configuration_change(cx);
        crate::app::dialogue::configure_selected_tts_model(Some(activation.selected));
        if activation.close_picker_on_success {
            self.voice_settings.finish_tts_picker();
        }
        self.complete_voice_setup_if_ready(cx);
    }
}
