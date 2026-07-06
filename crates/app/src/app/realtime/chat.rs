#[cfg(test)]
use super::acquire_process_slot;
use super::{
    release_process_slot, stop_sidecar_process_in_background, RealtimeControlState,
    RealtimeStartError,
};
#[cfg(test)]
use crate::app::voice::playback::AssistantAudioSink;
use gpui::Task;
use realtime::{
    PcmAudioChunk, RealtimeCommand, RealtimeEvent, RealtimeSessionConfig, RealtimeSessionId,
    RealtimeSidecarError, RealtimeStatus, SidecarLaunchConfig, SidecarProcess,
};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use voice_sherpa::{PcmStreamSession, SherpaProvider};

pub(crate) trait RealtimeSidecarClient: Send {
    fn send(&mut self, command: &RealtimeCommand) -> Result<(), RealtimeSidecarError>;
    fn try_recv(&self) -> Option<Result<RealtimeEvent, RealtimeSidecarError>>;
    fn stop(self: Box<Self>, timeout: Duration) -> Result<(), RealtimeSidecarError>;
}

impl RealtimeSidecarClient for SidecarProcess {
    fn send(&mut self, command: &RealtimeCommand) -> Result<(), RealtimeSidecarError> {
        SidecarProcess::send(self, command)
    }

    fn try_recv(&self) -> Option<Result<RealtimeEvent, RealtimeSidecarError>> {
        SidecarProcess::try_recv(self)
    }

    fn stop(self: Box<Self>, timeout: Duration) -> Result<(), RealtimeSidecarError> {
        (*self).stop(timeout)
    }
}

pub(crate) trait RealtimePcmStream: Send {
    fn stop(self: Box<Self>);
}

impl RealtimePcmStream for PcmStreamSession {
    fn stop(self: Box<Self>) {
        (*self).cancel();
    }
}

pub(crate) struct RealtimeChatRuntime {
    pub(crate) sidecar: Option<Box<dyn RealtimeSidecarClient>>,
    pub(crate) pcm_stream: Option<Box<dyn RealtimePcmStream>>,
    pub(crate) pcm_chunks: Option<async_channel::Receiver<PcmAudioChunk>>,
}

impl RealtimeChatRuntime {
    pub(crate) fn install_into(
        mut self,
        state: &mut RealtimeControlState,
    ) -> async_channel::Receiver<PcmAudioChunk> {
        state.sidecar = self.sidecar.take();
        state.pcm_stream = self.pcm_stream.take();
        self.pcm_chunks
            .take()
            .expect("chat runtime should retain PCM chunks until install")
    }

    pub(crate) fn dispose(&mut self) {
        self.pcm_chunks = None;
        if let Some(pcm_stream) = self.pcm_stream.take() {
            stop_pcm_stream_in_background(pcm_stream);
        }
        if let Some(sidecar) = self.sidecar.take() {
            stop_sidecar_process_in_background(sidecar);
        }
    }
}

impl Drop for RealtimeChatRuntime {
    fn drop(&mut self) {
        self.dispose();
    }
}

pub(crate) fn start_realtime_chat_runtime(
    config: RealtimeSessionConfig,
    sidecar_config: SidecarLaunchConfig,
    paraformer_model_dir: Option<PathBuf>,
    input_device_id: Option<String>,
) -> Result<RealtimeChatRuntime, RealtimeStartError> {
    voice_sherpa::request_microphone_permission().map_err(RealtimeStartError::Speech)?;
    let provider = match paraformer_model_dir {
        Some(path) => SherpaProvider::new(&path),
        None => SherpaProvider::from_env_or_default(),
    }
    .map_err(RealtimeStartError::Speech)?;
    let (pcm_stream, pcm_chunks) = provider
        .start_pcm_stream(input_device_id.as_deref())
        .map_err(RealtimeStartError::Speech)?;

    match spawn_and_start_sidecar(config, sidecar_config) {
        Ok(sidecar) => Ok(RealtimeChatRuntime {
            sidecar: Some(sidecar),
            pcm_stream: Some(Box::new(pcm_stream)),
            pcm_chunks: Some(pcm_chunks),
        }),
        Err(error) => {
            pcm_stream.cancel();
            Err(RealtimeStartError::Sidecar(error))
        }
    }
}

pub(super) fn spawn_and_start_sidecar(
    config: RealtimeSessionConfig,
    sidecar_config: SidecarLaunchConfig,
) -> Result<Box<dyn RealtimeSidecarClient>, RealtimeSidecarError> {
    let mut sidecar: Box<dyn RealtimeSidecarClient> =
        Box::new(SidecarProcess::spawn(sidecar_config)?);
    if let Err(error) = sidecar.send(&RealtimeCommand::StartSession(config)) {
        stop_sidecar_process_in_background(sidecar);
        return Err(error);
    }
    Ok(sidecar)
}

impl RealtimeControlState {
    pub(crate) fn set_startup_task(&mut self, task: Task<()>) {
        self.startup_task = Some(task);
    }

    pub(crate) fn set_sidecar_drain_task(&mut self, task: Task<()>) {
        self.sidecar_drain_task = Some(task);
    }

    pub(crate) fn set_pcm_forward_task(&mut self, task: Task<()>) {
        self.pcm_forward_task = Some(task);
    }

    pub(crate) fn mark_start_error(&mut self, error: RealtimeStartError) {
        if let Some(session_id) = self.session.session_id().cloned() {
            release_process_slot(&session_id);
        }
        self.stop_chat_runtime();
        self.stop_sidecar_process();
        self.session.mark_error(error.to_string());
        self.config_error = None;
    }

    pub(crate) fn send_input_audio_chunk(
        &mut self,
        chunk: PcmAudioChunk,
    ) -> Result<bool, RealtimeSidecarError> {
        let Some(session_id) = self.session.session_id().cloned() else {
            return Ok(false);
        };
        if self.sidecar.is_none() {
            return Ok(false);
        }

        if self.session.status() == RealtimeStatus::AssistantSpeaking {
            self.assistant_audio_sink.stop();
            self.send_sidecar_command(&RealtimeCommand::Interrupt {
                session_id: session_id.clone(),
            })?;
            self.session.mark_assistant_speaking(false);
        }

        self.session.mark_user_speaking(true);
        self.send_sidecar_command(&RealtimeCommand::InputAudioChunk { session_id, chunk })?;
        Ok(true)
    }

    pub(super) fn apply_sidecar_event(&mut self, event: RealtimeEvent) {
        match event {
            RealtimeEvent::SessionStarted { session_id } => {
                if self.matches_active_session(&session_id) {
                    self.session.mark_started();
                }
            }
            RealtimeEvent::AssistantAudioState {
                session_id,
                speaking,
            } => {
                if self.matches_active_session(&session_id) {
                    if speaking {
                        self.session.mark_user_speaking(false);
                    }
                    self.session.mark_assistant_speaking(speaking);
                }
            }
            RealtimeEvent::AssistantAudioChunk { session_id, chunk } => {
                if self.matches_active_session(&session_id) {
                    self.assistant_audio_sink.write_chunk(&chunk);
                }
            }
            RealtimeEvent::SessionError { message, .. } => {
                self.mark_sidecar_error(message);
            }
            RealtimeEvent::SessionStopped { .. } => {
                self.finish_sidecar_stopped();
            }
            RealtimeEvent::TranscriptDelta { .. }
            | RealtimeEvent::TranscriptFinal { .. }
            | RealtimeEvent::AssistantTextDelta { .. }
            | RealtimeEvent::ToolCall(_)
            | RealtimeEvent::IgnoredUnknown { .. } => {}
        }
    }

    pub(super) fn stop_chat_runtime(&mut self) {
        self.startup_task = None;
        self.sidecar_drain_task = None;
        self.pcm_forward_task = None;
        self.pending_startup_attempt = None;
        if let Some(pcm_stream) = self.pcm_stream.take() {
            stop_pcm_stream_in_background(pcm_stream);
        }
        self.assistant_audio_sink.stop();
    }

    fn send_sidecar_command(
        &mut self,
        command: &RealtimeCommand,
    ) -> Result<(), RealtimeSidecarError> {
        let Some(sidecar) = self.sidecar.as_mut() else {
            return Ok(());
        };
        sidecar.send(command)
    }

    fn matches_active_session(&self, session_id: &RealtimeSessionId) -> bool {
        self.session.session_id() == Some(session_id)
    }

    #[cfg(test)]
    pub(crate) fn start_shell_with_sidecar_for_tests(
        &mut self,
        config: RealtimeSessionConfig,
        sidecar: Box<dyn RealtimeSidecarClient>,
    ) -> Result<(), RealtimeStartError> {
        self.start_shell_with_sidecar_client_for_tests(config, sidecar)
    }

    #[cfg(test)]
    pub(crate) fn start_shell_with_sidecar_and_pcm_for_tests(
        &mut self,
        config: RealtimeSessionConfig,
        sidecar: Box<dyn RealtimeSidecarClient>,
        pcm_stream: Box<dyn RealtimePcmStream>,
    ) -> Result<(), RealtimeStartError> {
        self.start_shell_with_sidecar_client_for_tests(config, sidecar)?;
        self.pcm_stream = Some(pcm_stream);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn has_pcm_stream_for_tests(&self) -> bool {
        self.pcm_stream.is_some()
    }

    #[cfg(test)]
    pub(crate) fn set_assistant_audio_sink_for_tests(&mut self, sink: Box<dyn AssistantAudioSink>) {
        self.assistant_audio_sink = sink;
    }

    #[cfg(test)]
    pub(crate) fn apply_sidecar_event_for_tests(&mut self, event: RealtimeEvent) {
        self.apply_sidecar_event(event);
    }

    #[cfg(test)]
    fn start_shell_with_sidecar_client_for_tests(
        &mut self,
        config: RealtimeSessionConfig,
        mut sidecar: Box<dyn RealtimeSidecarClient>,
    ) -> Result<(), RealtimeStartError> {
        if self.is_active() {
            return self
                .session
                .start_connecting(config.session_id)
                .map_err(RealtimeStartError::Session);
        }

        let session_id = config.session_id.clone();
        acquire_process_slot(&session_id).map_err(RealtimeStartError::Session)?;
        if let Err(error) = self.session.start_connecting(config.session_id.clone()) {
            release_process_slot(&session_id);
            return Err(RealtimeStartError::Session(error));
        }
        self.config_error = None;

        if let Err(error) = sidecar.send(&RealtimeCommand::StartSession(config)) {
            release_process_slot(&session_id);
            self.session.mark_error(error.to_string());
            stop_sidecar_process_in_background(sidecar);
            return Err(RealtimeStartError::Sidecar(error));
        }

        self.sidecar = Some(sidecar);
        Ok(())
    }
}

fn stop_pcm_stream_in_background(pcm_stream: Box<dyn RealtimePcmStream>) {
    thread::spawn(move || {
        pcm_stream.stop();
    });
}
