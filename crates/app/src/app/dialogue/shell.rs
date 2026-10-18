use super::chat::start_dialogue_listening_runtime;
use crate::app::MultisoulApp;
use gpui::Context;
use std::time::Duration;

impl MultisoulApp {
    pub(crate) fn dialogue_voice_label(&self) -> Option<String> {
        self.dialogue.dialogue_voice_label()
    }

    pub(crate) fn effective_voice_label(&self) -> String {
        if self.voice_settings.values().desktop_mode()
            == crate::settings::voice::DesktopVoiceMode::Doubao
        {
            return self.doubao.label().into();
        }
        if self.dialogue_voice_label().is_some() {
            self.dialogue.effective_voice_label(self.voice_label())
        } else {
            self.voice_label()
        }
    }

    pub(crate) fn effective_voice_visual(&self) -> crate::app::voice::control::VoiceVisual {
        if self.voice_settings.values().desktop_mode()
            == crate::settings::voice::DesktopVoiceMode::Doubao
        {
            return match self.doubao.status {
                crate::app::doubao::DoubaoStatus::Idle => {
                    crate::app::voice::control::VoiceVisual::Idle
                }
                crate::app::doubao::DoubaoStatus::Connecting => {
                    crate::app::voice::control::VoiceVisual::Processing
                }
                crate::app::doubao::DoubaoStatus::Connected => {
                    if self.doubao.waiting_for_input() {
                        crate::app::voice::control::VoiceVisual::Idle
                    } else {
                        crate::app::voice::control::VoiceVisual::Recording
                    }
                }
                crate::app::doubao::DoubaoStatus::Thinking
                | crate::app::doubao::DoubaoStatus::UserTranscript(_)
                | crate::app::doubao::DoubaoStatus::AssistantReply(_)
                | crate::app::doubao::DoubaoStatus::RecordingResolved(_)
                | crate::app::doubao::DoubaoStatus::Speaking => {
                    crate::app::voice::control::VoiceVisual::Processing
                }
                crate::app::doubao::DoubaoStatus::Disconnected(_) => {
                    crate::app::voice::control::VoiceVisual::Error
                }
            };
        }
        self.dialogue.effective_voice_visual(self.voice.visual())
    }

    pub(crate) fn effective_voice_dock_label(&self) -> &'static str {
        if self.voice_settings.values().desktop_mode()
            == crate::settings::voice::DesktopVoiceMode::Doubao
        {
            return self.doubao.label();
        }
        match self.dialogue.phase() {
            dialogue::DialoguePhase::Thinking => ui::i18n::text("doubao.thinking"),
            dialogue::DialoguePhase::Speaking => ui::i18n::text("chrome.replying_continuous"),
            dialogue::DialoguePhase::Idle | dialogue::DialoguePhase::Listening => {
                self.effective_voice_visual().dock_label()
            }
        }
    }

    pub(crate) fn can_open_microphone_privacy_settings(&self) -> bool {
        self.voice.microphone_settings_recovery_available()
            || self.dialogue.microphone_settings_recovery_available()
            || self
                .voice_settings
                .requirement_state(crate::settings::voice::VoiceRequirement::Microphone)
                == crate::settings::voice::VoiceRequirementState::Failed
    }

    pub(crate) fn start_continuous_listening(&mut self, cx: &mut Context<Self>) {
        self.cancel_agent_announcements();
        self.dismiss_voice_error();
        self.voice.reset_caption_to_default();
        let values = self.voice_settings.values().clone();
        let model_root = self.voice_settings.model_root().to_path_buf();
        let store = self
            .voice_settings
            .credential_store()
            .expect("app voice settings need SQLite");
        let input_device_id = values.input_device_id().map(ToOwned::to_owned);
        let startup_attempt = self.dialogue.begin_startup_attempt();
        tracing::info!(
            target: "norma_voice",
            startup_attempt,
            selected_model = values
                .asr_model_id()
                .map(asr::AsrModelId::as_str)
                .unwrap_or("<none>"),
            model_root = %model_root.display(),
            input_device_selected = input_device_id.is_some(),
            "realtime voice startup requested"
        );
        let task = cx.spawn(async move |this, cx| {
            let runtime_task = cx.background_executor().spawn(async move {
                start_dialogue_listening_runtime(values, model_root, input_device_id, store)
            });
            let runtime = runtime_task.await;
            let _ = this.update(cx, |app, cx| {
                let should_accept = app.dialogue.should_accept_startup_attempt(startup_attempt);
                match runtime {
                    Ok(runtime) => {
                        if should_accept {
                            tracing::info!(
                                target: "norma_voice",
                                startup_attempt,
                                "realtime voice runtime ready; installing event pump"
                            );
                            app.dialogue.install_listening_runtime(runtime, cx);
                        } else {
                            tracing::info!(
                                startup_attempt,
                                "dropping stale dialogue listening startup"
                            );
                            runtime.dispose();
                        }
                    }
                    Err(error) => {
                        if should_accept {
                            tracing::warn!(
                                target: "norma_voice",
                                startup_attempt,
                                %error,
                                "realtime voice startup failed"
                            );
                            app.dialogue.mark_start_error(error);
                        } else {
                            tracing::info!(
                                startup_attempt,
                                %error,
                                "ignoring stale dialogue listening startup failure"
                            );
                        }
                    }
                }
                app.dialogue.finish_startup_attempt(startup_attempt);
                cx.notify();
            });
        });
        self.dialogue.set_startup_task(task);
        cx.notify();
    }

    pub(crate) fn stop_continuous_listening(&mut self, cx: &mut Context<Self>) {
        tracing::info!(target: "norma_voice", "realtime voice stop requested");
        self.dialogue.stop_shell();
        cx.notify();
    }

    pub(crate) fn start_dialogue_turn_loop(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(12))
                .await;
            let go = this.update_in(cx, |app, window, cx| {
                if app.dialogue.take_pending_voice_turn() {
                    app.dialogue.begin_voice_turn_reply(window, cx);
                } else if app.agent_announcement_audio_idle() {
                    app.dialogue.begin_queued_agent_report(window, cx);
                }
            });
            if go.is_err() {
                break;
            }
        })
        .detach();
    }
}
