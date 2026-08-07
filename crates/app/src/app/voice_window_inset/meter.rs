use audio::PcmAudioChunk;
use std::time::Duration;

const ENERGY_WINDOW_MILLIS: u64 = 16;
const RMS_SILENCE_FLOOR: f32 = 0.015;
const RMS_SPEECH_CEILING: f32 = 0.18;
const ENERGY_CURVE: f32 = 0.7;

#[derive(Clone, Copy, Debug)]
pub(crate) struct PcmEnergyFrame {
    pub(crate) energy: f32,
    pub(crate) duration: Duration,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PcmEnergyTimeline {
    frames: Vec<PcmEnergyFrame>,
}

impl PcmEnergyTimeline {
    pub(crate) fn from_pcm(chunk: &PcmAudioChunk) -> Self {
        let frame_count = chunk.bytes.len() / 2 / usize::from(chunk.channels);
        let mut frames = Vec::new();
        let mut frame_start = 0usize;
        let mut window_index = 0u64;

        while frame_start < frame_count {
            window_index = window_index.saturating_add(1);
            let window_end = ((window_index
                .saturating_mul(u64::from(chunk.sample_rate_hz))
                .saturating_mul(ENERGY_WINDOW_MILLIS))
                / 1_000) as usize;
            let frame_end = window_end.clamp(frame_start.saturating_add(1), frame_count);
            let samples = &chunk.bytes[frame_start * 2..frame_end * 2];
            frames.push(PcmEnergyFrame {
                energy: pcm16_rms_energy(samples),
                duration: Duration::from_secs_f64(
                    (frame_end - frame_start) as f64 / f64::from(chunk.sample_rate_hz),
                ),
            });
            frame_start = frame_end;
        }

        Self { frames }
    }

    pub(crate) fn into_frames(self) -> Vec<PcmEnergyFrame> {
        self.frames
    }
}

fn pcm16_rms_energy(bytes: &[u8]) -> f32 {
    if bytes.is_empty() {
        return 0.0;
    }
    let sum_of_squares = bytes
        .chunks_exact(2)
        .map(|sample| f32::from(i16::from_le_bytes([sample[0], sample[1]])) / 32_768.0)
        .map(|sample| sample * sample)
        .sum::<f32>();
    let rms = (sum_of_squares / (bytes.len() / 2) as f32).sqrt();
    ((rms - RMS_SILENCE_FLOOR) / (RMS_SPEECH_CEILING - RMS_SILENCE_FLOOR))
        .clamp(0.0, 1.0)
        .powf(ENERGY_CURVE)
}
