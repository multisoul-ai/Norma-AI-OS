#[cfg(test)]
use super::acquire_process_slot;
use super::{
    release_process_slot, stop_sidecar_process_in_background, RealtimeControlState,
    RealtimeStartError, SIDECAR_STOP_TIMEOUT,
};
#[cfg(test)]
use crate::app::voice::playback::AssistantAudioSink;
use crossbeam_channel::{unbounded, Receiver, RecvTimeoutError, Sender};
use gpui::Task;
use realtime::{
    PcmAudioChunk, RealtimeCallbackUpdate, RealtimeCommand, RealtimeEvent, RealtimeSessionConfig,
    RealtimeSessionId, RealtimeSidecarError, RealtimeStatus, SidecarLaunchConfig, SidecarProcess,
};
use std::io;
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use voice_sherpa::{PcmStreamSession, SherpaProvider};

pub(crate) trait RealtimeSidecarClient: Send {
    fn enqueue(&self, command: RealtimeCommand) -> Result<(), RealtimeSidecarError>;
    fn try_recv(&self) -> Option<Result<RealtimeEvent, RealtimeSidecarError>>;
    fn stop(self: Box<Self>, timeout: Duration) -> Result<(), RealtimeSidecarError>;
}

enum SidecarWorkerMessage {
    Command(RealtimeCommand),
    Stop {
        timeout: Duration,
        response: Sender<Result<(), RealtimeSidecarError>>,
    },
}

struct BackgroundSidecarClient {
    commands: Sender<SidecarWorkerMessage>,
    events: Receiver<Result<RealtimeEvent, RealtimeSidecarError>>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
}

impl BackgroundSidecarClient {
    fn spawn(sidecar: SidecarProcess) -> Self {
        let (command_tx, command_rx) = unbounded();
        let (event_tx, event_rx) = unbounded();
        let worker = thread::spawn(move || run_sidecar_worker(sidecar, command_rx, event_tx));
        Self {
            commands: command_tx,
            events: event_rx,
            worker: Mutex::new(Some(worker)),
        }
    }
}

impl RealtimeSidecarClient for BackgroundSidecarClient {
    fn enqueue(&self, command: RealtimeCommand) -> Result<(), RealtimeSidecarError> {
        self.commands
            .send(SidecarWorkerMessage::Command(command))
            .map_err(|_| closed_command_channel_error())
    }

    fn try_recv(&self) -> Option<Result<RealtimeEvent, RealtimeSidecarError>> {
        self.events.try_recv().ok()
    }

    fn stop(self: Box<Self>, timeout: Duration) -> Result<(), RealtimeSidecarError> {
        let (response_tx, response_rx) = unbounded();
        let _ = self.commands.send(SidecarWorkerMessage::Stop {
            timeout,
            response: response_tx,
        });

        let stop_result = response_rx.recv().unwrap_or(Ok(()));
        if let Some(worker) = self
            .worker
            .lock()
            .expect("background sidecar worker lock should not be poisoned")
            .take()
        {
            let _ = worker.join();
        }
        stop_result
    }
}

fn run_sidecar_worker(
    mut sidecar: SidecarProcess,
    command_rx: Receiver<SidecarWorkerMessage>,
    event_tx: Sender<Result<RealtimeEvent, RealtimeSidecarError>>,
) {
    loop {
        let mut should_exit = false;
        while let Some(event) = sidecar.try_recv() {
            let is_error = event.is_err();
            if event_tx.send(event).is_err() {
                should_exit = true;
                break;
            }
            if is_error {
                should_exit = true;
                break;
            }
        }
        if should_exit {
            let _ = sidecar.stop(SIDECAR_STOP_TIMEOUT);
            return;
        }

        match command_rx.recv_timeout(Duration::from_millis(10)) {
            Ok(SidecarWorkerMessage::Command(command)) => {
                if let Err(error) = sidecar.send(&command) {
                    let _ = event_tx.send(Err(error));
                    let _ = sidecar.stop(SIDECAR_STOP_TIMEOUT);
                    return;
                }
            }
            Ok(SidecarWorkerMessage::Stop { timeout, response }) => {
                let result = sidecar.stop(timeout);
                let _ = response.send(result);
                return;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                let _ = sidecar.stop(SIDECAR_STOP_TIMEOUT);
                return;
            }
        }
    }
}

fn closed_command_channel_error() -> RealtimeSidecarError {
    RealtimeSidecarError::CommandWriteFailed(io::Error::new(
        io::ErrorKind::BrokenPipe,
        "realtime sidecar command worker is closed",
    ))
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
    let sidecar: Box<dyn RealtimeSidecarClient> = Box::new(BackgroundSidecarClient::spawn(
        SidecarProcess::spawn(sidecar_config)?,
    ));
    if let Err(error) = sidecar.enqueue(RealtimeCommand::StartSession(config)) {
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

    pub(crate) fn enqueue_input_audio_chunk(
        &mut self,
        chunk: PcmAudioChunk,
    ) -> Result<bool, RealtimeSidecarError> {
        self.send_input_audio_chunk(chunk)
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
            self.send_sidecar_command(RealtimeCommand::Interrupt {
                session_id: session_id.clone(),
            })?;
            self.session.mark_assistant_speaking(false);
        }

        self.session.mark_user_speaking(true);
        self.send_sidecar_command(RealtimeCommand::InputAudioChunk { session_id, chunk })?;
        Ok(true)
    }

    pub(super) fn apply_sidecar_event(&mut self, event: RealtimeEvent) -> RealtimeCallbackUpdate {
        let mut callback_update = RealtimeCallbackUpdate::default();
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
                        self.session.mark_assistant_speaking(true);
                    } else {
                        callback_update = self.session.handle_assistant_audio_end();
                    }
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
            RealtimeEvent::TranscriptFinal { session_id, .. } => {
                if self.matches_active_session(&session_id) {
                    callback_update = self.session.handle_transcript_final();
                }
            }
            RealtimeEvent::TranscriptDelta { .. }
            | RealtimeEvent::AssistantTextDelta { .. }
            | RealtimeEvent::ToolCall(_)
            | RealtimeEvent::IgnoredUnknown { .. } => {}
        }
        callback_update
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

    pub(super) fn send_sidecar_command(
        &self,
        command: RealtimeCommand,
    ) -> Result<(), RealtimeSidecarError> {
        let Some(sidecar) = self.sidecar.as_ref() else {
            return Ok(());
        };
        sidecar.enqueue(command)
    }

    pub(super) fn take_sidecar_event(&self) -> Option<Result<RealtimeEvent, RealtimeSidecarError>> {
        self.sidecar.as_ref().and_then(|sidecar| sidecar.try_recv())
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
        let update = self.apply_sidecar_event(event);
        self.dispatch_tool_result_callbacks(update);
    }

    #[cfg(test)]
    fn start_shell_with_sidecar_client_for_tests(
        &mut self,
        config: RealtimeSessionConfig,
        sidecar: Box<dyn RealtimeSidecarClient>,
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

        if let Err(error) = sidecar.enqueue(RealtimeCommand::StartSession(config)) {
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
