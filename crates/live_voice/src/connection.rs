mod output;
mod transmit;
use transmit::transmit;

use crate::{
    providers::openai, session::validate_audio, AudioPacer, LiveCommand, LiveConfig, LiveError,
    LiveEvent, LiveSession, LiveUsage,
};
use audio::PcmAudioChunk;
use futures::{Sink, SinkExt, StreamExt};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    sync::{mpsc, OwnedSemaphorePermit, Semaphore},
    task::JoinHandle,
    time::{timeout, Instant},
};
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

// Each direction admits at most 16000 × 2 × 1 second of queued PCM.
const AUDIO_QUEUE_BYTES: usize = 32_000;
const START_TIMEOUT: Duration = Duration::from_secs(10);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(1);

struct AudioFrame {
    pcm: PcmAudioChunk,
    generation: u64,
    synthetic_silence: bool,
    _permit: OwnedSemaphorePermit,
}

/// Local transport observation, not a service acknowledgement or a playback receipt.
pub struct UploadedAudio {
    pub pcm: PcmAudioChunk,
    pub start_sample: u64,
    pub input_generation: u64,
    _permit: OwnedSemaphorePermit,
}

#[derive(Clone, Default)]
struct InputGate {
    output: Arc<output::OutputGate>,
    generation: u64,
    muted: bool,
    observer: Option<mpsc::Sender<UploadedAudio>>,
}

struct UploadObserverGuard(Arc<Mutex<InputGate>>);
impl Drop for UploadObserverGuard {
    fn drop(&mut self) {
        if let Ok(mut gate) = self.0.lock() {
            gate.observer = None;
        }
    }
}

struct EventFrame {
    output_epoch: u64,
    event: LiveEvent,
    _permit: Option<OwnedSemaphorePermit>,
}

#[derive(Clone)]
pub struct LiveControl {
    audio: mpsc::Sender<AudioFrame>,
    commands: mpsc::Sender<String>,
    session: Arc<Mutex<LiveSession>>,
    budget: Arc<Semaphore>,
    input_gate: Arc<Mutex<InputGate>>,
    cancellation: CancellationToken,
}

impl LiveControl {
    /// Subscribe before sending PCM. The sole consumer must drain these bounded observations.
    pub fn observe_uploaded_audio(&self) -> Result<mpsc::Receiver<UploadedAudio>, LiveError> {
        let mut gate = self.input_gate.lock().expect("Live input gate lock");
        if gate.observer.is_some() || self.cancellation.is_cancelled() {
            return Err(LiveError::NotReady);
        }
        let (sender, receiver) = mpsc::channel(64);
        gate.observer = Some(sender);
        Ok(receiver)
    }

    pub fn send_command(&self, command: &LiveCommand) -> Result<(), LiveError> {
        if self.cancellation.is_cancelled() {
            return Err(LiveError::NotReady);
        }
        let mut gate = self.input_gate.lock().expect("Live input gate lock");
        let message = self
            .session
            .lock()
            .expect("Live session lock")
            .command_message(command)?;
        self.commands
            .try_send(message)
            .map_err(|_| LiveError::Capacity)?;
        if let LiveCommand::Mute { muted, .. } = command {
            gate.muted = *muted;
            gate.generation = gate.generation.wrapping_add(1);
        }
        Ok(())
    }

    pub fn send_audio(&self, pcm: PcmAudioChunk) -> Result<(), LiveError> {
        self.enqueue_audio(pcm, false)
    }

    /// Preserve real-time cadence during privacy mute without reading microphone samples.
    pub fn send_silence(&self, samples: u32) -> Result<(), LiveError> {
        if samples == 0 || samples > 16_000 {
            return Err(LiveError::InvalidAudio);
        }
        let pcm = PcmAudioChunk::new(
            audio::PcmFormat::Pcm16,
            16_000,
            1,
            vec![0; samples as usize * 2],
        )
        .map_err(|_| LiveError::InvalidAudio)?;
        self.enqueue_audio(pcm, true)
    }

    fn enqueue_audio(&self, pcm: PcmAudioChunk, synthetic_silence: bool) -> Result<(), LiveError> {
        if self.cancellation.is_cancelled() {
            return Err(LiveError::NotReady);
        }
        validate_audio(&pcm)?;
        let gate = self.input_gate.lock().expect("Live input gate lock");
        if gate.muted && !synthetic_silence {
            return Ok(());
        }
        let permit = Arc::clone(&self.budget)
            .try_acquire_many_owned(pcm.bytes.len() as u32)
            .map_err(|_| LiveError::Capacity)?;
        self.audio
            .try_send(AudioFrame {
                pcm,
                generation: gate.generation,
                synthetic_silence,
                _permit: permit,
            })
            .map_err(|_| LiveError::Capacity)
    }

    pub fn queued_audio_bytes(&self) -> usize {
        AUDIO_QUEUE_BYTES - self.budget.available_permits()
    }

    pub fn close(&self) {
        self.cancellation.cancel();
    }
}

pub struct LiveConnection {
    session_id: String,
    control: LiveControl,
    events: mpsc::Receiver<EventFrame>,
    task: Option<JoinHandle<LiveOutcome>>,
}

#[derive(Debug)]
pub struct LiveOutcome {
    pub usage: LiveUsage,
    pub reason: Option<String>,
    pub error: Option<LiveError>,
}

pub async fn connect(
    config: LiveConfig,
    api_key: &str,
    cancellation: CancellationToken,
) -> Result<LiveConnection, LiveError> {
    connect_at(openai::ENDPOINT, false, config, api_key, cancellation).await
}

#[cfg(feature = "test-support")]
pub async fn connect_loopback(
    address: std::net::SocketAddr,
    config: LiveConfig,
    api_key: &str,
    cancellation: CancellationToken,
) -> Result<LiveConnection, LiveError> {
    if !address.ip().is_loopback() {
        return Err(LiveError::Connection);
    }
    connect_at(
        &format!("http://{address}/v1/live/sessions"),
        true,
        config,
        api_key,
        cancellation,
    )
    .await
}

async fn connect_at(
    endpoint: &str,
    loopback: bool,
    config: LiveConfig,
    api_key: &str,
    cancellation: CancellationToken,
) -> Result<LiveConnection, LiveError> {
    let startup = async {
        let mut socket = openai::connect(endpoint, api_key, loopback).await?;
        let mut session = LiveSession::new(config);
        socket
            .send(Message::Text(session.start_message()?.into()))
            .await
            .map_err(|_| LiveError::Connection)?;
        loop {
            let message = socket
                .next()
                .await
                .ok_or(LiveError::Connection)?
                .map_err(|_| LiveError::Connection)?;
            if let Message::Text(text) = message {
                if matches!(session.receive(&text)?, LiveEvent::Rejected { .. }) {
                    return Err(LiveError::ServerRejected);
                }
                if let Some(id) = session.session_id() {
                    return Ok::<_, LiveError>((socket, id.to_owned(), session));
                }
            }
        }
    };
    let (socket, session_id, session) = tokio::select! {
        biased;
        _ = cancellation.cancelled() => return Err(LiveError::Cancelled),
        result = timeout(START_TIMEOUT, startup) => result.map_err(|_| LiveError::Timeout)??,
    };
    let (audio, input) = mpsc::channel(64);
    let (commands, incoming_commands) = mpsc::channel(16);
    let (output, events) = mpsc::channel(64);
    let session = Arc::new(Mutex::new(session));
    let input_gate = Arc::new(Mutex::new(InputGate::default()));
    let control = LiveControl {
        audio,
        commands,
        session: Arc::clone(&session),
        budget: Arc::new(Semaphore::new(AUDIO_QUEUE_BYTES)),
        input_gate: Arc::clone(&input_gate),
        cancellation: cancellation.clone(),
    };
    let task = tokio::spawn(run(
        socket,
        session,
        input,
        incoming_commands,
        output,
        input_gate,
        cancellation,
    ));
    Ok(LiveConnection {
        session_id,
        control,
        events,
        task: Some(task),
    })
}

impl LiveConnection {
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn control(&self) -> LiveControl {
        self.control.clone()
    }

    /// Parsed events waiting for the application, within the 64-event queue budget.
    pub fn queued_events(&self) -> usize {
        self.events.len()
    }

    pub fn send_audio(&self, pcm: PcmAudioChunk) -> Result<(), LiveError> {
        self.control.send_audio(pcm)
    }

    pub async fn next_event(&mut self) -> Option<LiveEvent> {
        while let Some(frame) = self.events.recv().await {
            if matches!(frame.event, LiveEvent::OutputAudio(_))
                && !self
                    .control
                    .input_gate
                    .lock()
                    .expect("Live gate lock")
                    .output
                    .is_current(frame.output_epoch)
            {
                continue;
            }
            return Some(frame.event);
        }
        None
    }

    pub async fn finish(mut self) -> LiveOutcome {
        self.control.close();
        self.task
            .take()
            .expect("Live connection owns its driver")
            .await
            .unwrap_or(LiveOutcome {
                usage: LiveUsage::default(),
                reason: None,
                error: Some(LiveError::Connection),
            })
    }
}

impl Drop for LiveConnection {
    fn drop(&mut self) {
        self.control.close();
    }
}

async fn run(
    socket: openai::Socket,
    session: Arc<Mutex<LiveSession>>,
    input: mpsc::Receiver<AudioFrame>,
    commands: mpsc::Receiver<String>,
    output: mpsc::Sender<EventFrame>,
    input_gate: Arc<Mutex<InputGate>>,
    cancellation: CancellationToken,
) -> LiveOutcome {
    let _observer_guard = UploadObserverGuard(input_gate.clone());
    let output_gate = input_gate.lock().expect("Live gate lock").output.clone();
    let budget = Arc::new(Semaphore::new(AUDIO_QUEUE_BYTES));
    let (writer, mut reader) = socket.split();
    let send = transmit(
        writer,
        Arc::clone(&session),
        input,
        commands,
        input_gate,
        cancellation.clone(),
    );
    tokio::pin!(send);
    let mut writer_done = false;
    let mut close_deadline = None;
    let mut reason = None;
    let result: Result<(), LiveError> = async {
        loop {
            let (message, output_epoch) = tokio::select! {
                biased;
                _ = cancellation.cancelled(), if close_deadline.is_none() => {
                    close_deadline = Some(Instant::now() + CLOSE_TIMEOUT);
                    continue;
                }
                _ = async { tokio::time::sleep_until(close_deadline.expect("closing has a deadline")).await }, if close_deadline.is_some() => return Err(LiveError::Timeout),
                result = &mut send, if !writer_done => {
                    result?;
                    writer_done = true;
                    continue;
                }
                message = reader.next() => (message.ok_or(LiveError::Connection)?.map_err(|_| LiveError::Connection)?, output_gate.epoch()),
            };
            let text = match message {
                Message::Text(text) => text,
                Message::Ping(_) | Message::Pong(_) => continue,
                _ => return Err(LiveError::Connection),
            };
            let event = session.lock().expect("Live session lock").receive(&text)?;
            if event == LiveEvent::Ignored {
                continue;
            }
            if let LiveEvent::Closed { reason: r, .. } = &event {
                reason = Some(r.clone());
                // finish() owns finalization even when the application no longer consumes events.
                let _ = output.try_send(EventFrame { event, output_epoch:0, _permit: None });
                return Ok(());
            }
            if close_deadline.is_some() || cancellation.is_cancelled() {
                continue;
            }
            if matches!(event, LiveEvent::OutputAudio(_)) && !output_gate.is_current(output_epoch) { continue; }
            let permit = match &event {
                LiveEvent::OutputAudio(pcm) => {
                    validate_audio(pcm)?;
                    Some(Arc::clone(&budget).try_acquire_many_owned(pcm.bytes.len() as u32).map_err(|_| LiveError::Capacity)?)
                }
                _ => None,
            };
            output.try_send(EventFrame { event, output_epoch, _permit: permit }).map_err(|_| LiveError::Capacity)?;
        }
    }.await;
    cancellation.cancel();
    let usage = session.lock().expect("Live session lock").usage();
    LiveOutcome {
        usage,
        reason,
        error: result.err(),
    }
}

#[cfg(test)]
#[path = "../tests/unit/upload_shutdown.rs"]
mod upload_shutdown;
