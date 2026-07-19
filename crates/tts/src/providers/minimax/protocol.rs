use super::MiniMaxTtsConfig;
use crate::{TtsError, TtsErrorKind};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub(super) struct TaskStart<'a> {
    event: &'static str,
    model: &'a str,
    language_boost: &'static str,
    voice_setting: VoiceSetting<'a>,
    audio_setting: AudioSetting,
}

#[derive(Serialize)]
struct VoiceSetting<'a> {
    voice_id: &'a str,
    speed: f32,
    vol: f32,
    pitch: i32,
}

#[derive(Serialize)]
struct AudioSetting {
    sample_rate: u32,
    format: &'static str,
    channel: u16,
}

impl<'a> TaskStart<'a> {
    pub(super) fn from_config(config: &'a MiniMaxTtsConfig) -> Self {
        Self {
            event: "task_start",
            model: config.model_id().as_str(),
            language_boost: "auto",
            voice_setting: VoiceSetting {
                voice_id: config.voice_id().as_str(),
                speed: 1.0,
                vol: 1.0,
                pitch: 0,
            },
            audio_setting: AudioSetting {
                sample_rate: 32_000,
                format: "pcm",
                channel: 1,
            },
        }
    }
}

#[derive(Serialize)]
pub(super) struct TaskContinue<'a> {
    event: &'static str,
    text: &'a str,
}

impl<'a> TaskContinue<'a> {
    pub(super) fn new(text: &'a str) -> Self {
        Self {
            event: "task_continue",
            text,
        }
    }
}

#[derive(Serialize)]
pub(super) struct TaskFinish {
    event: &'static str,
}

impl TaskFinish {
    pub(super) fn new() -> Self {
        Self {
            event: "task_finish",
        }
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct IncomingFrame {
    pub(super) event: Option<String>,
    pub(super) session_id: Option<String>,
    pub(super) trace_id: Option<String>,
    pub(super) data: Option<AudioData>,
    pub(super) extra_info: Option<ExtraInfo>,
    #[serde(default)]
    pub(super) is_final: bool,
    pub(super) base_resp: Option<BaseResponse>,
}

#[derive(Debug, Deserialize)]
pub(super) struct AudioData {
    pub(super) audio: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ExtraInfo {
    pub(super) audio_sample_rate: Option<u32>,
    pub(super) audio_format: Option<String>,
    pub(super) audio_channel: Option<u16>,
    pub(super) usage_characters: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct BaseResponse {
    pub(super) status_code: i64,
}

pub(super) fn parse_incoming(text: &str) -> Result<IncomingFrame, TtsError> {
    let frame: IncomingFrame = serde_json::from_str(text).map_err(|_| {
        TtsError::new(
            TtsErrorKind::Protocol,
            "MiniMax returned an invalid protocol frame",
        )
    })?;
    if let Some(base) = frame.base_resp.as_ref() {
        if base.status_code != 0 {
            return Err(provider_status_error(base.status_code));
        }
    }
    Ok(frame)
}

pub(super) fn encode_message(value: &impl Serialize) -> Result<String, TtsError> {
    serde_json::to_string(value).map_err(|_| {
        TtsError::new(
            TtsErrorKind::Protocol,
            "could not encode MiniMax protocol message",
        )
    })
}

pub(super) fn provider_status_error(status_code: i64) -> TtsError {
    if status_code == 1008 {
        return TtsError::new(
            TtsErrorKind::QuotaExceeded,
            "MiniMax account balance is insufficient. Add balance or switch to an On-device voice.",
        );
    }
    let kind = match status_code {
        1004 | 2049 => TtsErrorKind::Authentication,
        1002 | 1039 | 1041 | 2056 | 2205 => TtsErrorKind::RateLimit,
        1042 | 2013 | 20132 | 2202 | 2203 | 2204 => TtsErrorKind::InvalidParameter,
        1001 | 1024 | 1033 | 2201 => TtsErrorKind::Transport,
        _ => TtsErrorKind::Protocol,
    };
    TtsError::new(
        kind,
        format!("MiniMax request failed with status {status_code}"),
    )
}
