//! Bounded PCM playback with completion based on actual WASAPI buffer consumption.
use super::{pcm_chunk_duration, AssistantAudioSink};
use audio::PcmAudioChunk;
use std::{collections::VecDeque, sync::mpsc, thread::JoinHandle, time::Duration};
#[path = "windows/device.rs"]
mod device;
use device::{Apartment, NativeOutput};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_QUEUED_BYTES: usize = 8 * 1024 * 1024;

trait OutputDevice {
    fn rate(&self) -> u32;
    fn capacity(&self) -> u32;
    fn frames_written(&self) -> u64;
    fn padding(&self) -> Result<u32, String>;
    fn write(&mut self, bytes: &[u8], frames: u32) -> Result<(), String>;
    fn stop(&mut self) -> Result<(), String>;
}

enum Command {
    Play(Pending, mpsc::Sender<Result<(), String>>),
    Stop(mpsc::Sender<Result<(), String>>),
    Shutdown,
}

struct Pending {
    chunk: PcmAudioChunk,
    offset: usize,
    first_frame: Option<u64>,
    last_frame: Option<u64>,
    started: Option<async_channel::Sender<()>>,
    completed: async_channel::Sender<()>,
}

pub(super) struct WindowsAssistantAudioSink {
    commands: Result<mpsc::Sender<Command>, String>,
    thread: Option<JoinHandle<()>>,
}

impl WindowsAssistantAudioSink {
    pub(super) fn new() -> Self {
        let (commands, incoming) = mpsc::channel();
        match std::thread::Builder::new()
            .name("assistant_audio_wasapi".into())
            .spawn(move || playback_loop(incoming))
        {
            Ok(thread) => Self {
                commands: Ok(commands),
                thread: Some(thread),
            },
            Err(error) => Self {
                commands: Err(format!(
                    "Windows audio: could not spawn playback worker: {error}"
                )),
                thread: None,
            },
        }
    }

    fn send(&self, command: Command) -> Result<(), String> {
        self.commands
            .as_ref()
            .map_err(Clone::clone)?
            .send(command)
            .map_err(|_| "Windows audio: playback worker is unavailable".into())
    }
}

impl AssistantAudioSink for WindowsAssistantAudioSink {
    fn write_chunk(&mut self, chunk: &PcmAudioChunk) -> Result<Duration, String> {
        let (started, _start_rx) = async_channel::bounded(1);
        let (completed, _complete_rx) = async_channel::bounded(1);
        self.write_chunk_with_completion(chunk, started, completed)
    }

    fn write_chunk_with_completion(
        &mut self,
        chunk: &PcmAudioChunk,
        started: async_channel::Sender<()>,
        completed: async_channel::Sender<()>,
    ) -> Result<Duration, String> {
        let chunk = PcmAudioChunk::new(
            chunk.format,
            chunk.sample_rate_hz,
            chunk.channels,
            chunk.bytes.clone(),
        )
        .map_err(|error| format!("Windows audio: invalid PCM chunk: {error:?}"))?;
        if chunk.bytes.is_empty() {
            return Err("Windows audio: cannot play an empty PCM chunk".into());
        }
        let duration = pcm_chunk_duration(&chunk);
        let (scheduled, result) = mpsc::channel();
        self.send(Command::Play(
            Pending {
                chunk,
                offset: 0,
                first_frame: None,
                last_frame: None,
                started: Some(started),
                completed,
            },
            scheduled,
        ))?;
        result
            .recv_timeout(COMMAND_TIMEOUT)
            .map_err(|_| "Windows audio: playback scheduling timed out".to_string())??;
        Ok(duration)
    }

    fn stop(&mut self) {
        let (stopped, _result) = mpsc::channel();
        if let Err(error) = self.send(Command::Stop(stopped)) {
            tracing::warn!(%error, "Windows playback stop was unavailable");
        }
    }

    fn stop_and_confirm(&mut self) -> Result<(), String> {
        let (stopped, result) = mpsc::channel();
        self.send(Command::Stop(stopped))?;
        result
            .recv_timeout(COMMAND_TIMEOUT)
            .map_err(|_| "Windows audio: playback stop timed out".to_string())?
    }
}

impl Drop for WindowsAssistantAudioSink {
    fn drop(&mut self) {
        let _ = self.send(Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                tracing::error!("Windows playback worker panicked");
            }
        }
    }
}

#[derive(Default)]
struct Playback {
    device: Option<Box<dyn OutputDevice>>,
    pending: VecDeque<Pending>,
    bytes: usize,
    failure: Option<String>,
}

impl Playback {
    fn enqueue(&mut self, pending: Pending) -> Result<(), String> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if pending.chunk.bytes.len() > MAX_QUEUED_BYTES.saturating_sub(self.bytes) {
            return Err("Windows audio: playback queue is full".into());
        }
        if self.pending.is_empty()
            && self
                .device
                .as_ref()
                .is_none_or(|device| device.rate() != pending.chunk.sample_rate_hz)
        {
            if let Some(mut old) = self.device.take() {
                old.stop()?;
            }
            self.device = Some(Box::new(NativeOutput::open(pending.chunk.sample_rate_hz)?));
        }
        self.bytes += pending.chunk.bytes.len();
        self.pending.push_back(pending);
        Ok(())
    }

    fn discard_last(&mut self) {
        if let Some(pending) = self.pending.pop_back() {
            self.bytes -= pending.chunk.bytes.len();
        }
    }

    fn tick(&mut self) -> Result<(), String> {
        let Some(device) = &self.device else {
            return Ok(());
        };
        let padding = device.padding()?;
        let consumed = device.frames_written() - u64::from(padding);
        self.retire_consumed(consumed);
        let Some(device) = &mut self.device else {
            return Ok(());
        };
        let Some(first) = self.pending.front() else {
            return Ok(());
        };
        if first.chunk.sample_rate_hz != device.rate() {
            // Different providers can use different rates; preserve the old tail first.
            if padding == 0 {
                device.stop()?;
                self.device = Some(Box::new(NativeOutput::open(first.chunk.sample_rate_hz)?));
            }
            return Ok(());
        }
        let mut available = device.capacity() - padding;
        for pending in &mut self.pending {
            if available == 0 || pending.chunk.sample_rate_hz != device.rate() {
                break;
            }
            let remaining = (pending.chunk.bytes.len() - pending.offset) / 2;
            let frames = remaining.min(available as usize) as u32;
            if frames == 0 {
                continue;
            }
            let end = pending.offset + frames as usize * 2;
            let first_frame = device.frames_written() + 1;
            device.write(&pending.chunk.bytes[pending.offset..end], frames)?;
            pending.first_frame.get_or_insert(first_frame);
            pending.offset = end;
            if end == pending.chunk.bytes.len() {
                pending.last_frame = Some(device.frames_written());
            }
            available -= frames;
        }
        Ok(())
    }

    fn retire_consumed(&mut self, consumed: u64) {
        // Notifications follow the native consumption cursor, including queue delay.
        for pending in &mut self.pending {
            if pending.first_frame.is_some_and(|frame| frame <= consumed) {
                if let Some(started) = pending.started.take() {
                    let _ = started.try_send(());
                }
            }
        }
        while self
            .pending
            .front()
            .is_some_and(|pending| pending.last_frame.is_some_and(|frame| frame <= consumed))
        {
            if let Some(pending) = self.pending.pop_front() {
                self.bytes -= pending.chunk.bytes.len();
                let _ = pending.completed.try_send(());
            }
        }
    }

    fn stop(&mut self) -> Result<(), String> {
        let result = self
            .device
            .take()
            .map_or(Ok(()), |mut device| device.stop());
        self.pending.clear();
        self.bytes = 0;
        if let Some(error) = self.failure.take() {
            Err(error)
        } else {
            result
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/windows_playback_queue_tests.rs"]
mod tests;

fn playback_loop(incoming: mpsc::Receiver<Command>) {
    let _apartment = match Apartment::new() {
        Ok(apartment) => apartment,
        Err(error) => {
            while let Ok(command) = incoming.recv() {
                match command {
                    Command::Play(_, result) | Command::Stop(result) => {
                        let _ = result.send(Err(error.clone()));
                    }
                    Command::Shutdown => break,
                }
            }
            return;
        }
    };
    let mut playback = Playback::default();
    loop {
        match incoming.recv_timeout(Duration::from_millis(5)) {
            Ok(Command::Play(pending, result)) => {
                let scheduled = playback.enqueue(pending);
                // A scheduling timeout must not leave late audio queued for playback.
                if matches!(result.send(scheduled), Err(mpsc::SendError(Ok(())))) {
                    playback.discard_last();
                }
            }
            Ok(Command::Stop(result)) => {
                let _ = result.send(playback.stop());
            }
            Ok(Command::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                if let Err(error) = playback.stop() {
                    tracing::warn!(%error, "Windows playback shutdown failed");
                }
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if let Err(error) = playback.tick() {
            tracing::error!(%error, "Windows audio output failed");
            let _ = playback.stop();
            playback.failure = Some(error);
        }
    }
}
