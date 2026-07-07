use crate::audio::{PcmAudioChunk, PcmAudioError, PcmFormat};
use crate::session::{RealtimeSessionConfig, RealtimeSessionId};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq)]
pub enum RealtimeCommand {
    StartSession(RealtimeSessionConfig),
    StopSession {
        session_id: RealtimeSessionId,
    },
    Interrupt {
        session_id: RealtimeSessionId,
    },
    InputAudioChunk {
        session_id: RealtimeSessionId,
        chunk: PcmAudioChunk,
    },
    ToolResult(RealtimeToolResult),
}

#[derive(Clone, Debug, PartialEq)]
pub enum RealtimeEvent {
    SessionStarted {
        session_id: RealtimeSessionId,
    },
    TranscriptDelta {
        session_id: RealtimeSessionId,
        text: String,
    },
    TranscriptFinal {
        session_id: RealtimeSessionId,
        text: String,
    },
    AssistantTextDelta {
        session_id: RealtimeSessionId,
        text: String,
    },
    AssistantAudioChunk {
        session_id: RealtimeSessionId,
        chunk: PcmAudioChunk,
    },
    AssistantAudioState {
        session_id: RealtimeSessionId,
        speaking: bool,
    },
    ToolCall(RealtimeToolCall),
    SessionError {
        session_id: RealtimeSessionId,
        message: String,
    },
    SessionStopped {
        session_id: RealtimeSessionId,
        reason: String,
    },
    IgnoredUnknown {
        event_type: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RealtimeToolCall {
    pub session_id: RealtimeSessionId,
    pub tool_call_id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RealtimeToolResult {
    pub session_id: RealtimeSessionId,
    pub tool_call_id: String,
    pub result: Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RealtimeToolResultStatus {
    Succeeded,
    Failed,
    Cancelled,
}

impl RealtimeToolResult {
    pub fn task_result(
        session_id: RealtimeSessionId,
        tool_call_id: impl Into<String>,
        status: RealtimeToolResultStatus,
        task_id: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            session_id,
            tool_call_id: tool_call_id.into(),
            result: json!({
                "status": status,
                "task_id": task_id.into(),
                "summary": summary.into(),
            }),
        }
    }

    pub fn failed(
        session_id: RealtimeSessionId,
        tool_call_id: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            session_id,
            tool_call_id: tool_call_id.into(),
            result: json!({
                "status": "failed",
                "summary": summary.into(),
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RealtimeProtocolError {
    #[error("malformed JSON line")]
    MalformedJson,
    #[error("command could not be encoded")]
    CommandEncodeFailed,
    #[error("audio data could not be decoded")]
    InvalidAudioData,
    #[error("audio chunk failed validation: {0:?}")]
    InvalidAudioChunk(PcmAudioError),
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum WireCommand<'a> {
    #[serde(rename = "start_session")]
    StartSession {
        session_id: &'a str,
        model: &'a str,
        tools: &'a [String],
    },
    #[serde(rename = "stop_session")]
    StopSession { session_id: &'a str },
    #[serde(rename = "interrupt")]
    Interrupt { session_id: &'a str },
    #[serde(rename = "input_audio_chunk")]
    InputAudioChunk {
        session_id: &'a str,
        format: PcmFormat,
        sample_rate: u32,
        channels: u16,
        data_b64: String,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        session_id: &'a str,
        tool_call_id: &'a str,
        result: &'a Value,
    },
}

#[derive(Deserialize)]
struct WireEventType {
    #[serde(rename = "type")]
    event_type: String,
}

#[derive(Deserialize)]
struct WireSessionEvent {
    session_id: RealtimeSessionId,
}

#[derive(Deserialize)]
struct WireTextEvent {
    session_id: RealtimeSessionId,
    text: String,
}

#[derive(Deserialize)]
struct WireAssistantAudioChunk {
    session_id: RealtimeSessionId,
    format: PcmFormat,
    sample_rate: u32,
    channels: Option<u16>,
    data_b64: String,
}

#[derive(Deserialize)]
struct WireAssistantAudioState {
    session_id: RealtimeSessionId,
    speaking: bool,
}

#[derive(Deserialize)]
struct WireToolCall {
    session_id: RealtimeSessionId,
    tool_call_id: String,
    name: String,
    arguments: Value,
}

#[derive(Deserialize)]
struct WireSessionError {
    session_id: RealtimeSessionId,
    message: String,
}

#[derive(Deserialize)]
struct WireSessionStopped {
    session_id: RealtimeSessionId,
    reason: String,
}

pub fn encode_command(command: &RealtimeCommand) -> Result<String, RealtimeProtocolError> {
    let wire = match command {
        RealtimeCommand::StartSession(config) => WireCommand::StartSession {
            session_id: config.session_id.as_str(),
            model: &config.model,
            tools: &config.tools,
        },
        RealtimeCommand::StopSession { session_id } => WireCommand::StopSession {
            session_id: session_id.as_str(),
        },
        RealtimeCommand::Interrupt { session_id } => WireCommand::Interrupt {
            session_id: session_id.as_str(),
        },
        RealtimeCommand::InputAudioChunk { session_id, chunk } => WireCommand::InputAudioChunk {
            session_id: session_id.as_str(),
            format: chunk.format,
            sample_rate: chunk.sample_rate_hz,
            channels: chunk.channels,
            data_b64: STANDARD.encode(&chunk.bytes),
        },
        RealtimeCommand::ToolResult(result) => WireCommand::ToolResult {
            session_id: result.session_id.as_str(),
            tool_call_id: &result.tool_call_id,
            result: &result.result,
        },
    };

    serde_json::to_string(&wire).map_err(|_| RealtimeProtocolError::CommandEncodeFailed)
}

pub fn decode_event(line: &str) -> Result<RealtimeEvent, RealtimeProtocolError> {
    let value: Value =
        serde_json::from_str(line).map_err(|_| RealtimeProtocolError::MalformedJson)?;
    let event_type = serde_json::from_value::<WireEventType>(value.clone())
        .map_err(|_| RealtimeProtocolError::MalformedJson)?
        .event_type;

    match event_type.as_str() {
        "session_started" => {
            let wire = decode_wire::<WireSessionEvent>(value)?;
            Ok(RealtimeEvent::SessionStarted {
                session_id: wire.session_id,
            })
        }
        "transcript_delta" => decode_text_event(value, |session_id, text| {
            RealtimeEvent::TranscriptDelta { session_id, text }
        }),
        "transcript_final" => decode_text_event(value, |session_id, text| {
            RealtimeEvent::TranscriptFinal { session_id, text }
        }),
        "assistant_text_delta" => decode_text_event(value, |session_id, text| {
            RealtimeEvent::AssistantTextDelta { session_id, text }
        }),
        "assistant_audio_chunk" => decode_assistant_audio_chunk(value),
        "assistant_audio_state" => {
            let wire = decode_wire::<WireAssistantAudioState>(value)?;
            Ok(RealtimeEvent::AssistantAudioState {
                session_id: wire.session_id,
                speaking: wire.speaking,
            })
        }
        "tool_call" => {
            let wire = decode_wire::<WireToolCall>(value)?;
            Ok(RealtimeEvent::ToolCall(RealtimeToolCall {
                session_id: wire.session_id,
                tool_call_id: wire.tool_call_id,
                name: wire.name,
                arguments: wire.arguments,
            }))
        }
        "session_error" => {
            let wire = decode_wire::<WireSessionError>(value)?;
            Ok(RealtimeEvent::SessionError {
                session_id: wire.session_id,
                message: wire.message,
            })
        }
        "session_stopped" => {
            let wire = decode_wire::<WireSessionStopped>(value)?;
            Ok(RealtimeEvent::SessionStopped {
                session_id: wire.session_id,
                reason: wire.reason,
            })
        }
        _ => Ok(RealtimeEvent::IgnoredUnknown { event_type }),
    }
}

fn decode_wire<T>(value: Value) -> Result<T, RealtimeProtocolError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(value).map_err(|_| RealtimeProtocolError::MalformedJson)
}

fn decode_text_event(
    value: Value,
    build: impl FnOnce(RealtimeSessionId, String) -> RealtimeEvent,
) -> Result<RealtimeEvent, RealtimeProtocolError> {
    let wire = decode_wire::<WireTextEvent>(value)?;
    Ok(build(wire.session_id, wire.text))
}

fn decode_assistant_audio_chunk(value: Value) -> Result<RealtimeEvent, RealtimeProtocolError> {
    let wire = decode_wire::<WireAssistantAudioChunk>(value)?;
    let bytes = STANDARD
        .decode(wire.data_b64.as_bytes())
        .map_err(|_| RealtimeProtocolError::InvalidAudioData)?;
    let chunk = PcmAudioChunk::new(
        wire.format,
        wire.sample_rate,
        wire.channels.unwrap_or(1),
        bytes,
    )
    .map_err(RealtimeProtocolError::InvalidAudioChunk)?;

    Ok(RealtimeEvent::AssistantAudioChunk {
        session_id: wire.session_id,
        chunk,
    })
}

#[cfg(test)]
#[path = "../tests/unit/protocol_tests.rs"]
mod tests;
