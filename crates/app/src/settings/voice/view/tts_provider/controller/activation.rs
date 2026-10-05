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
                self.voice_settings.report_tts_status(
                    ui::i18n::LocalizedText::new(
                        "voice.choose_a_tts_provider_before_applying_a_voice",
                    ),
                    true,
                );
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
                ui::i18n::LocalizedText::new(
                    "voice.connect_minimax_and_choose_a_verified_voice_before_applying",
                ),
                true,
            );
            cx.notify();
            return;
        }
        let rollback = self.voice_settings.capture_tts_activation_rollback();
        self.voice_settings.begin_tts_picker_apply();
        if !self.voice_settings.commit_tts_picker_minimax_profile() {
            self.voice_settings
                .fail_tts_picker_apply(ui::i18n::LocalizedText::new(
                    "voice.connect_minimax_and_choose_a_verified_voice_before_applying",
                ));
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
            self.voice_settings.report_tts_status(
                ui::i18n::LocalizedText::new(
                    "voice.choose_a_ready_on_device_voice_before_applying",
                ),
                true,
            );
            cx.notify();
            return;
        };
        self.voice_settings.begin_tts_picker_apply();
        self.apply_tts_picker_model(model_id, voice_id, cx);
    }

    fn report_tts_activation_save_wait(&mut self, cx: &mut Context<Self>) {
        self.voice_settings.report_tts_status(
            ui::i18n::LocalizedText::new(
                "voice.wait_for_the_current_settings_save_to_finish_before_applying_this",
            ),
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
                .fail_tts_picker_apply(ui::i18n::LocalizedText::new(
                    "voice.minimax_configuration_is_incomplete",
                ));
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
        self.clear_agent_announcement_audio_for_voice_change();
        self.tts_preview.cancel();
        self.stop_continuous_listening(cx);
        self.voice.stop_for_model_switch();
        self.invalidate_tts_for_configuration_change(cx);
        self.dialogue.selected_tts_model = Some(activation.selected);
        if activation.close_picker_on_success {
            self.voice_settings.finish_tts_picker();
        }
        self.complete_voice_setup_if_ready(cx);
    }
}
