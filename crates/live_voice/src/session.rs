use crate::{DelegationNotice, Speaker, TranscriptFragment};
use audio::{PcmAudioChunk, PcmFormat};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::json;
use std::collections::BTreeMap;
use thiserror::Error;
mod command;
pub use command::{Acknowledgement, AppendKind, LiveCommand};

// 16kHz × 1声道 × 2字节 × 1秒；跨包积压由传输与播放队列另行限制。
pub const MAX_PCM_CHUNK_BYTES: usize = 32_000;

#[derive(Clone, Debug)]
pub struct LiveConfig {
    pub instructions: String,
    pub voice: String,
}

impl Default for LiveConfig {
    fn default() -> Self {
        Self {
            instructions: String::new(),
            voice: "marin".into(),
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum LiveError {
    #[error("Live session is not ready for this command")]
    NotReady,
    #[error("Invalid Live event")]
    InvalidEvent,
    #[error("Live requires mono 16kHz PCM16 chunks of at most one second")]
    InvalidAudio,
    #[error("Live connection failed")]
    Connection,
    #[error("Live authentication rejected (HTTP {0})")]
    Authentication(u16),
    #[error("Live connection or finalization timed out")]
    Timeout,
    #[error("Live connection cancelled")]
    Cancelled,
    #[error("Live audio or event queue exceeded its bounded capacity")]
    Capacity,
    #[error("Live session startup rejected by the service")]
    ServerRejected,
    #[error("Invalid Live control or context command")]
    InvalidCommand,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LiveEvent {
    OutputAudio(PcmAudioChunk),
    Transcript(TranscriptFragment),
    Delegation(DelegationNotice),
    Acknowledged {
        client_event_id: String,
        kind: Acknowledgement,
    },
    Rejected {
        code: Option<String>,
        client_event_id: Option<String>,
    },
    Usage(LiveUsage),
    Closed {
        usage: LiveUsage,
        reason: String,
    },
    Ignored,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LiveUsage {
    pub seconds: f64,
    pub finalized: bool,
}

pub struct LiveSession {
    config: LiveConfig,
    phase: Phase,
    usage: LiveUsage,
    session_id: Option<String>,
    delegations: BTreeMap<String, f64>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Phase {
    New,
    Starting,
    Active,
    Closing,
    Closed,
}

impl LiveSession {
    pub fn new(config: LiveConfig) -> Self {
        Self {
            config,
            phase: Phase::New,
            usage: LiveUsage::default(),
            session_id: None,
            delegations: BTreeMap::new(),
        }
    }

    pub fn start_message(&mut self) -> Result<String, LiveError> {
        if self.phase != Phase::New {
            return Err(LiveError::NotReady);
        }
        self.phase = Phase::Starting;
        Ok(json!({"type":"session.start","session":{
            "model":"gpt-live-1","instructions":self.config.instructions,
            "audio":{"format":{"type":"audio/pcm","rate":16000},
                "output":{"voice":self.config.voice}},
            "delegation":{"type":"client"},"store":false
        }})
        .to_string())
    }

    pub fn receive(&mut self, message: &str) -> Result<LiveEvent, LiveError> {
        let event = serde_json::from_str::<serde_json::Value>(message)
            .map_err(|_| LiveError::InvalidEvent)?;
        if let Some(ack) = command::acknowledgement(&event)? {
            return Ok(if self.phase == Phase::Active {
                ack
            } else {
                LiveEvent::Ignored
            });
        }
        if event["type"] == "session.delegation.created" {
            if self.phase != Phase::Active || event["delegation"]["target"] != "client" {
                return Ok(LiveEvent::Ignored);
            }
            let id = event["delegation"]["id"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 256)
                .ok_or(LiveError::InvalidEvent)?
                .to_owned();
            let offset_ms = event["offset_ms"]
                .as_f64()
                .filter(|v| v.is_finite() && *v >= 0.0)
                .ok_or(LiveError::InvalidEvent)?;
            if event["delegation"]["type"] != "delegation" {
                return Err(LiveError::InvalidEvent);
            }
            if let Some(previous) = self.delegations.get(&id) {
                return if *previous == offset_ms {
                    Ok(LiveEvent::Ignored)
                } else {
                    Err(LiveError::InvalidEvent)
                };
            }
            // Active task identities are never silently evicted; reconnect with verified state at the bound.
            if self.delegations.len() >= 256 {
                return Err(LiveError::Capacity);
            }
            self.delegations.insert(id.clone(), offset_ms);
            return Ok(LiveEvent::Delegation(DelegationNotice { id, offset_ms }));
        }
        if event["type"] == "error" {
            let code = event["error"]["code"]
                .as_str()
                .filter(|s| {
                    !s.is_empty()
                        && s.len() <= 128
                        && s.bytes()
                            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
                })
                .map(str::to_owned);
            let client_event_id = event["error"]["client_event_id"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 256)
                .map(str::to_owned);
            return Ok(LiveEvent::Rejected {
                code,
                client_event_id,
            });
        }
        if matches!(
            event["type"].as_str(),
            Some("session.input_transcript.delta" | "session.output_transcript.delta")
        ) {
            if self.phase != Phase::Active {
                return Ok(LiveEvent::Ignored);
            }
            let start_ms = event["start_ms"]
                .as_f64()
                .filter(|v| v.is_finite() && *v >= 0.0)
                .ok_or(LiveError::InvalidEvent)?;
            let end_ms = event["end_ms"]
                .as_f64()
                .filter(|v| v.is_finite() && *v >= start_ms)
                .ok_or(LiveError::InvalidEvent)?;
            let event_id = event["event_id"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 256)
                .ok_or(LiveError::InvalidEvent)?
                .to_owned();
            let delta = event["delta"]
                .as_str()
                .filter(|s| s.len() <= 16_384)
                .ok_or(LiveError::InvalidEvent)?
                .to_owned();
            let speaker = if event["type"] == "session.input_transcript.delta" {
                Speaker::User
            } else {
                Speaker::Assistant
            };
            return Ok(LiveEvent::Transcript(TranscriptFragment {
                event_id,
                speaker,
                delta,
                start_ms,
                end_ms,
            }));
        }
        if event["type"] == "session.started" {
            if self.phase != Phase::Starting
                || event["session"]["model"] != "gpt-live-1"
                || event["session"]["id"].as_str().is_none_or(str::is_empty)
            {
                return Err(LiveError::InvalidEvent);
            }
            self.phase = Phase::Active;
            self.session_id = event["session"]["id"].as_str().map(str::to_owned);
        }
        if event["type"] == "session.usage.updated" || event["type"] == "session.closed" {
            if !matches!(self.phase, Phase::Active | Phase::Closing) {
                return Err(LiveError::NotReady);
            }
            let seconds = event["usage"]["seconds"]
                .as_f64()
                .filter(|v| v.is_finite() && *v >= self.usage.seconds)
                .ok_or(LiveError::InvalidEvent)?;
            let finalized = event["type"] == "session.closed";
            if finalized {
                if event["session"]["id"].as_str() != self.session_id.as_deref() {
                    return Err(LiveError::InvalidEvent);
                }
                let reason = event["reason"]
                    .as_str()
                    .filter(|v| !v.is_empty())
                    .ok_or(LiveError::InvalidEvent)?
                    .to_owned();
                self.phase = Phase::Closed;
                self.usage = LiveUsage { seconds, finalized };
                return Ok(LiveEvent::Closed {
                    usage: self.usage,
                    reason,
                });
            }
            self.usage = LiveUsage { seconds, finalized };
            return Ok(LiveEvent::Usage(self.usage));
        }
        if event["type"] == "session.output_audio.delta" {
            if self.phase == Phase::Closing {
                return Ok(LiveEvent::Ignored);
            }
            if self.phase != Phase::Active {
                return Err(LiveError::NotReady);
            }
            let data = STANDARD
                .decode(event["delta"].as_str().ok_or(LiveError::InvalidEvent)?)
                .map_err(|_| LiveError::InvalidEvent)?;
            let pcm = PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, data)
                .map_err(|_| LiveError::InvalidEvent)?;
            return Ok(LiveEvent::OutputAudio(pcm));
        }
        Ok(LiveEvent::Ignored)
    }

    pub fn audio_message(&self, pcm: &PcmAudioChunk) -> Result<String, LiveError> {
        if self.phase != Phase::Active {
            return Err(LiveError::NotReady);
        }
        validate_audio(pcm)?;
        Ok(
            json!({"type":"session.input_audio.append","audio":STANDARD.encode(&pcm.bytes)})
                .to_string(),
        )
    }

    pub fn close_message(&mut self) -> Result<String, LiveError> {
        if self.phase != Phase::Active {
            return Err(LiveError::NotReady);
        }
        self.phase = Phase::Closing;
        Ok(json!({"type":"session.close"}).to_string())
    }

    pub fn usage(&self) -> LiveUsage {
        self.usage
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }
}

pub(crate) fn validate_audio(pcm: &PcmAudioChunk) -> Result<(), LiveError> {
    if pcm.sample_rate_hz != 16_000
        || pcm.channels != 1
        || pcm.format != PcmFormat::Pcm16
        || pcm.bytes.is_empty()
        || !pcm.bytes.len().is_multiple_of(2)
        || pcm.bytes.len() > MAX_PCM_CHUNK_BYTES
    {
        return Err(LiveError::InvalidAudio);
    }
    Ok(())
}
