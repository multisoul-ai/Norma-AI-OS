use super::chat::start_realtime_chat_runtime;
use super::{RealtimeControlState, SIDECAR_POLL_INTERVAL};
use crate::app::voice::control::VoiceVisual;
use crate::app::MultisoulApp;
use gpui::Context;
use realtime::PcmAudioChunk;

impl MultisoulApp {
    pub(crate) fn realtime_voice_label(&self) -> Option<String> {
        self.realtime.realtime_voice_label()
    }

    pub(crate) fn effective_voice_label(&self) -> String {
        if self.realtime_voice_label().is_some() {
            self.realtime.effective_voice_label(self.voice_label())
        } else {
            self.voice_label()
        }
    }

    pub(crate) fn effective_voice_visual(&self) -> VoiceVisual {
        self.realtime.effective_voice_visual(self.voice.visual())
    }

    pub(crate) fn start_always_listening_shell(&mut self, cx: &mut Context<Self>) {
        match RealtimeControlState::validate_start_config(self.voice_settings.values()) {
            Ok(config) => {
                let sidecar_config =
                    RealtimeControlState::sidecar_launch_config(self.voice_settings.values());
                if let Some(sidecar_config) = sidecar_config {
                    let paraformer_model_dir = self
                        .voice_settings
                        .values()
                        .paraformer_model_dir()
                        .map(ToOwned::to_owned);
                    let input_device_id = self
                        .voice_settings
                        .values()
                        .input_device_id()
                        .map(ToOwned::to_owned);
                    match self.realtime.start_shell_with_config(config.clone()) {
                        Ok(()) => {
                            let startup_attempt = self.realtime.begin_chat_startup_attempt();
                            let expected_session_id = config.session_id.clone();
                            if let Some(session_id) = self.realtime.current_session_id() {
                                tracing::info!(%session_id, "starting always listening chat shell");
                            }
                            let task = cx.spawn(async move |this, cx| {
                                let runtime_task = cx.background_executor().spawn(async move {
                                    start_realtime_chat_runtime(
                                        config,
                                        sidecar_config,
                                        paraformer_model_dir,
                                        input_device_id,
                                    )
                                });
                                let runtime = runtime_task.await;
                                let _ = this.update(cx, |app, cx| {
                                    let should_accept =
                                        app.realtime.should_accept_chat_startup_attempt(
                                            startup_attempt,
                                            &expected_session_id,
                                        );
                                    match runtime {
                                        Ok(mut runtime) => {
                                            if should_accept {
                                                let pcm_chunks =
                                                    runtime.install_into(&mut app.realtime);
                                                app.start_realtime_pcm_forward_loop(pcm_chunks, cx);
                                                app.start_realtime_sidecar_drain_loop(cx);
                                            } else {
                                                tracing::info!(
                                                    %expected_session_id,
                                                    startup_attempt,
                                                    "dropping stale realtime chat startup"
                                                );
                                                runtime.dispose();
                                            }
                                        }
                                        Err(error) => {
                                            if should_accept {
                                                tracing::warn!(
                                                    %error,
                                                    "failed to start realtime chat shell"
                                                );
                                                app.realtime.mark_start_error(error);
                                            } else {
                                                tracing::info!(
                                                    %expected_session_id,
                                                    startup_attempt,
                                                    %error,
                                                    "ignoring stale realtime chat startup failure"
                                                );
                                            }
                                        }
                                    }
                                    app.realtime.finish_chat_startup_attempt(startup_attempt);
                                    cx.notify();
                                });
                            });
                            self.realtime.set_startup_task(task);
                        }
                        Err(error) => {
                            tracing::warn!(%error, "failed to start realtime shell");
                        }
                    }
                } else if self.realtime.start_shell_with_config(config).is_ok() {
                    if let Some(session_id) = self.realtime.current_session_id() {
                        tracing::info!(%session_id, "started always listening shell");
                    }
                    cx.spawn(async move |this, cx| {
                        let _ = this.update(cx, |app, cx| {
                            app.realtime.mark_started();
                            cx.notify();
                        });
                    })
                    .detach();
                }
            }
            Err(error) => self.realtime.mark_config_error(error),
        }
        cx.notify();
    }

    fn start_realtime_sidecar_drain_loop(&mut self, cx: &mut Context<Self>) {
        let task = cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(SIDECAR_POLL_INTERVAL).await;
            let go = this.update(cx, |app, cx| {
                let changed = app.realtime.drain_sidecar_events();
                if changed {
                    cx.notify();
                }
                app.realtime.has_sidecar() && app.realtime.is_active()
            });
            match go {
                Ok(true) => {}
                Ok(false) | Err(_) => break,
            }
        });
        self.realtime.set_sidecar_drain_task(task);
    }

    fn start_realtime_pcm_forward_loop(
        &mut self,
        pcm_chunks: async_channel::Receiver<PcmAudioChunk>,
        cx: &mut Context<Self>,
    ) {
        let task = cx.spawn(async move |this, cx| {
            while let Ok(chunk) = pcm_chunks.recv().await {
                let go = this.update(cx, |app, cx| {
                    match app.realtime.send_input_audio_chunk(chunk) {
                        Ok(sent) => {
                            if sent {
                                cx.notify();
                            }
                            app.realtime.has_sidecar() && app.realtime.is_active()
                        }
                        Err(error) => {
                            app.realtime.mark_sidecar_error(error.to_string());
                            cx.notify();
                            false
                        }
                    }
                });
                match go {
                    Ok(true) => {}
                    Ok(false) | Err(_) => break,
                }
            }
        });
        self.realtime.set_pcm_forward_task(task);
    }

    pub(crate) fn stop_always_listening_shell(&mut self, cx: &mut Context<Self>) {
        self.realtime.stop_shell();
        cx.notify();
    }
}
