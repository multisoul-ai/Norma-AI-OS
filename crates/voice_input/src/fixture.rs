//! Debug-build-only microphone replacement for desktop E2E.
//!
//! With `NORMA_E2E_AUDIO_FIXTURE=<wav>` the PCM capture entry points replay that
//! file at real-time pace (then silence) instead of opening the microphone, and
//! report microphone permission as granted. The real device is never touched, so
//! no TCC prompt can appear. Compiled only when `debug_assertions` is on; release
//! builds contain none of this.
use crate::audio_capture::{pcm_stream_chunk_from_samples, PCM_STREAM_SAMPLE_RATE_HZ};
use crate::resample::LinearResampler;
use crate::stream::PcmStreamSession;
use crate::VoiceInputError;
use audio::PcmAudioChunk;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

pub(crate) const FIXTURE_ENV: &str = "NORMA_E2E_AUDIO_FIXTURE";
const CHUNK_MS: u64 = 100;
const CHUNK_SAMPLES: usize = (PCM_STREAM_SAMPLE_RATE_HZ as usize) * (CHUNK_MS as usize) / 1000;
const CHUNK_QUEUE: usize = 64;

#[cfg(test)]
#[path = "../tests/unit/fixture_tests.rs"]
mod tests;

pub(crate) fn fixture_path_from(value: Option<String>) -> Option<PathBuf> {
    value.filter(|value| !value.is_empty()).map(PathBuf::from)
}

pub(crate) fn fixture_path() -> Option<PathBuf> {
    fixture_path_from(std::env::var(FIXTURE_ENV).ok())
}

fn audio_error(path: &Path, detail: impl std::fmt::Display) -> VoiceInputError {
    VoiceInputError::Audio(format!("E2E audio fixture {}: {detail}", path.display()))
}

/// Reads a WAV (16-bit int or 32-bit float) as 16 kHz mono f32.
pub(crate) fn decode_fixture(path: &Path) -> Result<Vec<f32>, VoiceInputError> {
    let mut reader = hound::WavReader::open(path).map_err(|error| audio_error(path, error))?;
    let spec = reader.spec();
    let channels = usize::from(spec.channels.max(1));
    let interleaved: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Int, 16) => reader
            .samples::<i16>()
            .map(|sample| sample.map(|value| f32::from(value) / 32_768.0))
            .collect::<Result<_, _>>(),
        (hound::SampleFormat::Float, 32) => reader.samples::<f32>().collect::<Result<_, _>>(),
        (format, bits) => {
            return Err(audio_error(
                path,
                format!("unsupported WAV format {format:?}/{bits} bit"),
            ))
        }
    }
    .map_err(|error| audio_error(path, error))?;
    let mono: Vec<f32> = interleaved
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect();
    if spec.sample_rate == PCM_STREAM_SAMPLE_RATE_HZ {
        return Ok(mono);
    }
    let mut resampler = LinearResampler::new(
        f64::from(spec.sample_rate),
        f64::from(PCM_STREAM_SAMPLE_RATE_HZ),
    );
    Ok(resampler.process(&mono))
}

pub(crate) fn chunks_100ms(samples: &[f32]) -> Vec<PcmAudioChunk> {
    samples
        .chunks(CHUNK_SAMPLES)
        .filter_map(pcm_stream_chunk_from_samples)
        .collect()
}

/// Replays the fixture in real time, then silence, until the session is dropped.
pub(crate) fn start_fixture_stream(
    path: &Path,
) -> Result<(PcmStreamSession, async_channel::Receiver<PcmAudioChunk>), VoiceInputError> {
    let chunks = chunks_100ms(&decode_fixture(path)?);
    let silence = pcm_stream_chunk_from_samples(&[0.0; CHUNK_SAMPLES])
        .ok_or_else(|| audio_error(path, "silence chunk could not be built"))?;
    let (chunk_tx, chunk_rx) = async_channel::bounded::<PcmAudioChunk>(CHUNK_QUEUE);
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let thread = std::thread::Builder::new()
        .name("voice_input_e2e_fixture".to_string())
        .spawn(move || {
            let mut queue = chunks.into_iter();
            // Real-time pace; a stop signal or a closed receiver ends the loop.
            while let Err(mpsc::RecvTimeoutError::Timeout) =
                stop_rx.recv_timeout(Duration::from_millis(CHUNK_MS))
            {
                let chunk = queue.next().unwrap_or_else(|| silence.clone());
                if matches!(
                    chunk_tx.try_send(chunk),
                    Err(async_channel::TrySendError::Closed(_))
                ) {
                    break;
                }
            }
        })
        .map_err(|error| audio_error(path, error))?;
    tracing::warn!(
        target: "norma_voice",
        fixture = %path.display(),
        "E2E audio fixture replaces the microphone"
    );
    Ok((PcmStreamSession::from_fixture(stop_tx, thread), chunk_rx))
}
