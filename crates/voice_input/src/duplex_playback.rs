use audio::PcmAudioChunk;
use std::sync::mpsc;
use std::time::Duration;

/// Voice-processing playback shares the capture engine. 48 kHz matches that graph.
pub(crate) const DUPLEX_OUTPUT_RATE_HZ: u32 = 48_000;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) enum DuplexCommand {
    Play {
        pcm: PcmAudioChunk,
        started: async_channel::Sender<()>,
        done: async_channel::Sender<()>,
        result: mpsc::Sender<Result<(), String>>,
    },
    StopPlayer(mpsc::Sender<Result<(), String>>),
}

/// Schedules assistant audio on the same engine that captures the microphone.
#[derive(Clone)]
pub struct DuplexPlayback {
    tx: mpsc::Sender<DuplexCommand>,
}

impl DuplexPlayback {
    pub(crate) fn from_sender(tx: mpsc::Sender<DuplexCommand>) -> Self {
        Self { tx }
    }

    pub fn play_with_completion(
        &self,
        pcm: &PcmAudioChunk,
        started: async_channel::Sender<()>,
        done: async_channel::Sender<()>,
    ) -> Result<Duration, String> {
        let (result_tx, result_rx) = mpsc::channel();
        self.tx
            .send(DuplexCommand::Play {
                pcm: pcm.clone(),
                started,
                done,
                result: result_tx,
            })
            .map_err(|_| "duplex playback is closed".to_string())?;
        result_rx
            .recv_timeout(COMMAND_TIMEOUT)
            .map_err(|_| "duplex playback scheduling timed out".to_string())??;
        Ok(pcm_duration(pcm))
    }

    pub fn stop(&self) -> Result<(), String> {
        let (done, stopped) = mpsc::channel();
        self.tx
            .send(DuplexCommand::StopPlayer(done))
            .map_err(|_| "duplex playback is closed".to_string())?;
        stopped
            .recv_timeout(COMMAND_TIMEOUT)
            .map_err(|_| "duplex playback stop timed out".to_string())?
    }
}

pub(crate) fn pcm_duration(pcm: &PcmAudioChunk) -> Duration {
    let channels = usize::from(pcm.channels.max(1));
    let frames = pcm.bytes.len() / 2 / channels;
    Duration::from_secs_f64(frames as f64 / f64::from(pcm.sample_rate_hz.max(1)))
}
