use super::chat::start_dialogue_listening_runtime;
use super::DialogueControlState;
use crate::app::MultisoulApp;
use gpui::Context;
use std::time::Duration;

impl MultisoulApp {
    pub(crate) fn dialogue_voice_label(&self) -> Option<String> {
        self.dialogue.dialogue_voice_label()
    }

    pub(crate) fn effective_voice_label(&self) -> String {
        if self.dialogue_voice_label().is_some() {
            self.dialogue.effective_voice_label(self.voice_label())
        } else {
            self.voice_label()
        }
    }

    pub(crate) fn effective_voice_visual(&self) -> crate::app::voice::control::VoiceVisual {
        self.dialogue.effective_voice_visual(self.voice.visual())
    }

    pub(crate) fn start_always_listening_shell(&mut self, cx: &mut Context<Self>) {
        match DialogueControlState::validate_dialogue_start(self.voice_settings.values()) {
            Ok(provider) => {
                let input_device_id = self
                    .voice_settings
                    .values()
                    .input_device_id()
                    .map(ToOwned::to_owned);
                let startup_attempt = self.dialogue.begin_startup_attempt();
                let task = cx.spawn(async move |this, cx| {
                    let runtime_task = cx.background_executor().spawn(async move {
                        start_dialogue_listening_runtime(provider, input_device_id)
                    });
                    let runtime = runtime_task.await;
                    let _ = this.update(cx, |app, cx| {
                        let should_accept = app.dialogue.should_accept_startup_attempt(startup_attempt);
                        match runtime {
                            Ok(runtime) => {
                                if should_accept {
                                    app.dialogue
                                        .install_listening_runtime(runtime, cx);
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
                                    tracing::warn!(%error, "failed to start dialogue listening shell");
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
            }
            Err(error) => {
                tracing::warn!(%error, "failed to validate dialogue listening shell");
                self.dialogue.mark_start_error(error);
            }
        }
        cx.notify();
    }

    pub(crate) fn stop_always_listening_shell(&mut self, cx: &mut Context<Self>) {
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
                }
            });
            if go.is_err() {
                break;
            }
        })
        .detach();
    }
}
