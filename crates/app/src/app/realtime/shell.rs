use super::chat::start_realtime_chat_runtime;
use super::{RealtimeConfigError, RealtimeControlState, SIDECAR_POLL_INTERVAL};
use crate::app::voice::control::VoiceVisual;
use crate::app::MultisoulApp;
use gpui::Context;
use realtime::{
    PcmAudioChunk, RealtimeCommand, RealtimeEvent, RealtimeToolCall, RealtimeToolResult,
};

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
                } else {
                    self.realtime
                        .mark_config_error(RealtimeConfigError::MissingSidecarPath);
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
                let changed = app.drain_realtime_sidecar(cx);
                if changed {
                    cx.notify();
                }
                app.realtime.has_sidecar() && app.realtime.is_active()
            });
            let task_windows = cx.update(|app| app.windows()).unwrap_or_default();
            let task_changed = task_windows
                .into_iter()
                .filter_map(|window| window.downcast::<MultisoulApp>())
                .any(|window| {
                    window
                        .update(cx, |app, window, cx| {
                            app.drain_realtime_card_tasks(window, cx)
                        })
                        .unwrap_or(false)
                });
            if task_changed {
                tracing::debug!("drained realtime card task tick");
            }
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
                    match app.realtime.enqueue_input_audio_chunk(chunk) {
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

    pub(crate) fn drain_realtime_sidecar(&mut self, cx: &mut Context<Self>) -> bool {
        let mut changed = false;
        loop {
            let event = match self.realtime.take_sidecar_event() {
                Some(event) => event,
                None => break,
            };

            changed = true;
            match event {
                Ok(RealtimeEvent::ToolCall(tool_call)) => {
                    self.route_realtime_tool_call(tool_call, cx);
                }
                Ok(event) => {
                    let terminal = matches!(
                        event,
                        RealtimeEvent::SessionError { .. } | RealtimeEvent::SessionStopped { .. }
                    );
                    let update = self.realtime.apply_sidecar_event(event);
                    self.realtime.dispatch_tool_result_callbacks(update);
                    if terminal {
                        break;
                    }
                }
                Err(error) => {
                    self.realtime.mark_sidecar_error(error.to_string());
                    break;
                }
            }
        }
        changed
    }

    fn route_realtime_tool_call(&mut self, tool_call: RealtimeToolCall, cx: &mut Context<Self>) {
        let RealtimeToolCall {
            session_id,
            tool_call_id,
            name,
            arguments,
        } = tool_call;

        match name.as_str() {
            "enqueue_card_task" => {
                let intent = arguments
                    .get("intent")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned);
                match intent {
                    Some(intent) => self.enqueue_realtime_card_task(intent, tool_call_id, cx),
                    None => self.send_failed_realtime_tool_result(
                        session_id,
                        tool_call_id,
                        "enqueue_card_task is missing intent",
                    ),
                }
            }
            "cancel_card_task" => self.cancel_realtime_card_task(cx),
            _ => self.send_failed_realtime_tool_result(
                session_id,
                tool_call_id,
                format!("unknown tool: {name}"),
            ),
        }
    }

    fn send_failed_realtime_tool_result(
        &mut self,
        session_id: realtime::RealtimeSessionId,
        tool_call_id: String,
        summary: impl Into<String>,
    ) {
        let command = RealtimeCommand::ToolResult(RealtimeToolResult::failed(
            session_id,
            tool_call_id,
            summary,
        ));
        if let Err(error) = self.realtime.send_sidecar_command(command) {
            tracing::warn!(%error, "failed to send realtime tool result");
        }
    }
}
