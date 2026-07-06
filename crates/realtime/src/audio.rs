use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PcmFormat {
    Pcm16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PcmAudioChunk {
    pub format: PcmFormat,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcmAudioError {
    InvalidSampleRate,
    InvalidChannels,
    InvalidPcm16Length,
}

impl PcmAudioChunk {
    pub fn new(
        format: PcmFormat,
        sample_rate_hz: u32,
        channels: u16,
        bytes: Vec<u8>,
    ) -> Result<Self, PcmAudioError> {
        if sample_rate_hz != 24_000 {
            return Err(PcmAudioError::InvalidSampleRate);
        }
        if channels != 1 {
            return Err(PcmAudioError::InvalidChannels);
        }
        if matches!(format, PcmFormat::Pcm16) && bytes.len() % 2 != 0 {
            return Err(PcmAudioError::InvalidPcm16Length);
        }

        Ok(Self {
            format,
            sample_rate_hz,
            channels,
            bytes,
        })
    }
}

#[cfg(test)]
#[path = "../tests/unit/audio_tests.rs"]
mod tests;
