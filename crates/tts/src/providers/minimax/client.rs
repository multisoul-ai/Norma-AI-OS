use super::protocol::{
    encode_message, parse_incoming, IncomingFrame, TaskContinue, TaskFinish, TaskStart,
};
use super::{MiniMaxConnection, MiniMaxTransport, MiniMaxTtsConfig, ProductionMiniMaxTransport};
use crate::{TtsChunkSink, TtsClient, TtsError, TtsErrorKind, TtsStreamSummary};
use audio::{PcmAudioChunk, PcmFormat};
use std::sync::Arc;
use std::time::{Duration, Instant};

const SETUP_ATTEMPTS: usize = 2;
const SETUP_TIMEOUT: Duration = Duration::from_secs(10);

struct ActiveSession {
    connection: Box<dyn MiniMaxConnection>,
    sink: Arc<dyn TtsChunkSink>,
    trace_id: Option<String>,
    session_id: Option<String>,
    usage_characters: u64,
    chunk_count: u64,
    byte_count: u64,
}

pub struct MiniMaxTtsClient {
    api_key: String,
    config: MiniMaxTtsConfig,
    transport: Arc<dyn MiniMaxTransport>,
    active: Option<ActiveSession>,
}

impl MiniMaxTtsClient {
    pub fn new(api_key: String, config: MiniMaxTtsConfig) -> Self {
        Self::new_with_transport(api_key, config, Arc::new(ProductionMiniMaxTransport))
    }

    pub(super) fn new_with_transport(
        api_key: String,
        config: MiniMaxTtsConfig,
        transport: Arc<dyn MiniMaxTransport>,
    ) -> Self {
        Self {
            api_key,
            config,
            transport,
            active: None,
        }
    }

    fn start_attempt(
        &self,
        sink: Arc<dyn TtsChunkSink>,
        deadline: Instant,
    ) -> Result<ActiveSession, TtsError> {
        let mut connection = self.transport.connect(&self.api_key, deadline)?;
        let connected = parse_incoming(&receive_required(&mut *connection, &sink, deadline)?)?;
        require_event(&connected, "connected_success")?;
        connection.set_setup_deadline(deadline)?;
        connection.send_text(encode_message(&TaskStart::from_config(&self.config))?)?;
        let started = parse_incoming(&receive_required(&mut *connection, &sink, deadline)?)?;
        require_event(&started, "task_started")?;
        if let Err(error) = connection.finish_setup() {
            connection.close();
            return Err(error.after_task_accepted());
        }
        Ok(ActiveSession {
            connection,
            sink,
            trace_id: started.trace_id.or(connected.trace_id),
            session_id: started.session_id.or(connected.session_id),
            usage_characters: 0,
            chunk_count: 0,
            byte_count: 0,
        })
    }

    fn fail_active(&mut self, error: TtsError) -> TtsError {
        let error = error.after_task_accepted();
        if let Some(mut active) = self.active.take() {
            active.connection.close();
            active.sink.finish(Err(error.clone()));
        }
        error
    }

    fn receive_until(&mut self, terminal_event: &str) -> Result<(), TtsError> {
        loop {
            let receive_result = {
                let active = self
                    .active
                    .as_mut()
                    .expect("active MiniMax session must exist while receiving");
                if active.sink.is_cancelled() {
                    Err(TtsError::new(
                        TtsErrorKind::Cancelled,
                        "TTS stream was cancelled",
                    ))
                } else {
                    active.connection.receive_text()
                }
            };
            let text = match receive_result {
                Ok(Some(text)) => text,
                Ok(None) => continue,
                Err(error) => return Err(self.fail_active(error)),
            };
            let frame = match parse_incoming(&text) {
                Ok(frame) => frame,
                Err(error) => return Err(self.fail_active(error)),
            };
            if let Err(error) = self.consume_frame_audio(&frame) {
                return Err(self.fail_active(error));
            }
            let event = frame.event.as_deref();
            if event == Some("task_failed") {
                return Err(self.fail_active(TtsError::new(
                    TtsErrorKind::Protocol,
                    "MiniMax synthesis task failed",
                )));
            }
            if terminal_event == "task_continued" {
                // MiniMax's documented Task Continued payload may omit `event`; `is_final`
                // is the authoritative boundary for one submitted text segment.
                if frame.is_final {
                    return Ok(());
                }
            } else if event == Some(terminal_event) {
                return Ok(());
            }
        }
    }

    fn consume_frame_audio(&mut self, frame: &IncomingFrame) -> Result<(), TtsError> {
        let Some(audio_hex) = frame
            .data
            .as_ref()
            .and_then(|data| data.audio.as_deref())
            .filter(|audio| !audio.is_empty())
        else {
            return Ok(());
        };
        if let Some(extra) = frame.extra_info.as_ref() {
            let unsupported_rate = extra.audio_sample_rate.is_some_and(|rate| rate != 32_000);
            let unsupported_channels = extra.audio_channel.is_some_and(|channels| channels != 1);
            let unsupported_format = extra
                .audio_format
                .as_deref()
                .is_some_and(|format| format != "pcm");
            if unsupported_rate || unsupported_channels || unsupported_format {
                return Err(TtsError::new(
                    TtsErrorKind::AudioDecode,
                    "MiniMax returned unsupported audio metadata",
                ));
            }
        }
        let bytes = hex::decode(audio_hex).map_err(|_| {
            TtsError::new(
                TtsErrorKind::AudioDecode,
                "MiniMax returned invalid hex audio",
            )
        })?;
        let chunk = PcmAudioChunk::new(PcmFormat::Pcm16, 32_000, 1, bytes).map_err(|_| {
            TtsError::new(
                TtsErrorKind::AudioDecode,
                "MiniMax returned invalid PCM audio",
            )
        })?;
        let active = self
            .active
            .as_mut()
            .expect("active MiniMax session must exist while consuming audio");
        if active.sink.is_cancelled() || !active.sink.send(chunk.clone()) {
            return Err(TtsError::new(
                TtsErrorKind::Cancelled,
                "TTS stream was cancelled",
            ));
        }
        active.chunk_count = active.chunk_count.saturating_add(1);
        active.byte_count = active.byte_count.saturating_add(chunk.bytes.len() as u64);
        active.usage_characters = active.usage_characters.saturating_add(
            frame
                .extra_info
                .as_ref()
                .and_then(|extra| extra.usage_characters)
                .unwrap_or(0),
        );
        if frame.trace_id.is_some() {
            active.trace_id = frame.trace_id.clone();
        }
        if frame.session_id.is_some() {
            active.session_id = frame.session_id.clone();
        }
        Ok(())
    }
}

impl TtsClient for MiniMaxTtsClient {
    fn start_stream(&mut self, sink: Arc<dyn TtsChunkSink>) -> Result<(), TtsError> {
        if self.active.is_some() {
            return Err(TtsError::new(
                TtsErrorKind::Configuration,
                "a MiniMax TTS stream is already active",
            ));
        }
        let mut last_error = None;
        let deadline = Instant::now() + SETUP_TIMEOUT;
        for attempt in 0..SETUP_ATTEMPTS {
            match self.start_attempt(Arc::clone(&sink), deadline) {
                Ok(session) => {
                    self.active = Some(session);
                    return Ok(());
                }
                Err(error) => {
                    let should_retry = error.kind() == TtsErrorKind::Transport
                        && !error.task_accepted()
                        && attempt + 1 < SETUP_ATTEMPTS;
                    last_error = Some(error);
                    if !should_retry {
                        break;
                    }
                }
            }
        }
        Err(last_error.expect("MiniMax setup always records an attempt error"))
    }

    fn push_text(&mut self, text: &str) -> Result<(), TtsError> {
        if text.trim().is_empty() {
            return Err(TtsError::new(
                TtsErrorKind::InvalidParameter,
                "TTS text cannot be blank",
            ));
        }
        let message = encode_message(&TaskContinue::new(text))?;
        if let Err(error) = self
            .active
            .as_mut()
            .ok_or_else(|| {
                TtsError::new(TtsErrorKind::Configuration, "no active MiniMax TTS stream")
            })?
            .connection
            .send_text(message)
        {
            return Err(self.fail_active(error));
        }
        self.receive_until("task_continued")
    }

    fn finish_stream(&mut self) -> Result<(), TtsError> {
        let message = encode_message(&TaskFinish::new())?;
        if let Err(error) = self
            .active
            .as_mut()
            .ok_or_else(|| {
                TtsError::new(TtsErrorKind::Configuration, "no active MiniMax TTS stream")
            })?
            .connection
            .send_text(message)
        {
            return Err(self.fail_active(error));
        }
        self.receive_until("task_finished")?;
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.chunk_count == 0)
        {
            return Err(self.fail_active(TtsError::new(
                TtsErrorKind::AudioDecode,
                "MiniMax completed without playable PCM audio",
            )));
        }
        let mut active = self
            .active
            .take()
            .expect("MiniMax session remains active until task_finished");
        active.connection.close();
        active.sink.finish(Ok(TtsStreamSummary::new(
            active.trace_id,
            active.session_id,
            active.usage_characters,
            active.chunk_count,
            active.byte_count,
        )));
        Ok(())
    }

    fn cancel_stream(&mut self) {
        if let Some(mut active) = self.active.take() {
            active.connection.close();
            active.sink.finish(Err(TtsError::new(
                TtsErrorKind::Cancelled,
                "TTS stream was cancelled",
            )));
        }
    }
}

fn require_event(frame: &IncomingFrame, expected: &str) -> Result<(), TtsError> {
    if frame.event.as_deref() == Some(expected) {
        Ok(())
    } else {
        Err(TtsError::new(
            TtsErrorKind::Protocol,
            format!("MiniMax did not return expected {expected} event"),
        ))
    }
}

fn receive_required(
    connection: &mut dyn MiniMaxConnection,
    sink: &Arc<dyn TtsChunkSink>,
    deadline: Instant,
) -> Result<String, TtsError> {
    loop {
        if sink.is_cancelled() {
            return Err(TtsError::new(
                TtsErrorKind::Cancelled,
                "TTS stream was cancelled",
            ));
        }
        if Instant::now() >= deadline {
            return Err(TtsError::new(
                TtsErrorKind::Transport,
                "MiniMax WebSocket setup timed out",
            )
            .retryable_before_acceptance());
        }
        if let Some(text) = connection.receive_text()? {
            return Ok(text);
        }
    }
}
