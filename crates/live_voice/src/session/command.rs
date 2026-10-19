use super::{LiveError, LiveEvent, LiveSession, Phase};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppendKind {
    Instructions,
    Thinking,
    Commentary,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LiveCommand {
    Mute {
        event_id: String,
        muted: bool,
    },
    Append {
        event_id: String,
        kind: AppendKind,
        delegation_id: Option<String>,
        content: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Acknowledgement {
    InputMuted,
    InputUnmuted,
    Appended(AppendKind),
}

impl LiveSession {
    pub fn command_message(&self, command: &LiveCommand) -> Result<String, LiveError> {
        if self.phase != Phase::Active {
            return Err(LiveError::NotReady);
        }
        let event_id = match command {
            LiveCommand::Mute { event_id, .. } | LiveCommand::Append { event_id, .. } => event_id,
        };
        if event_id.is_empty() || event_id.len() > 128 {
            return Err(LiveError::InvalidCommand);
        }
        let body = match command {
            LiveCommand::Mute { muted, .. } => {
                json!({"type":if *muted { "session.input_audio.mute" } else { "session.input_audio.unmute" },"event_id":event_id})
            }
            LiveCommand::Append {
                kind,
                delegation_id,
                content,
                ..
            } => {
                // shortcut: 500 UTF-8 bytes conservatively bound 500 tokens; app splits longer summaries.
                if content.trim().is_empty()
                    || content.len() > 500
                    || delegation_id
                        .as_ref()
                        .is_some_and(|id| !self.delegations.contains_key(id))
                {
                    return Err(LiveError::InvalidCommand);
                }
                let event_type = match kind {
                    AppendKind::Instructions => "session.instructions.append",
                    AppendKind::Thinking => "session.thinking.append",
                    AppendKind::Commentary => "session.commentary.append",
                };
                json!({"type":event_type,"event_id":event_id,"delegation_id":delegation_id,"content":content})
            }
        };
        Ok(body.to_string())
    }
}

pub(super) fn acknowledgement(event: &Value) -> Result<Option<LiveEvent>, LiveError> {
    let kind = match event["type"].as_str() {
        Some("session.input_audio.muted") => Acknowledgement::InputMuted,
        Some("session.input_audio.unmuted") => Acknowledgement::InputUnmuted,
        Some("session.instructions.appended") => {
            Acknowledgement::Appended(AppendKind::Instructions)
        }
        Some("session.thinking.appended") => Acknowledgement::Appended(AppendKind::Thinking),
        Some("session.commentary.appended") => Acknowledgement::Appended(AppendKind::Commentary),
        _ => return Ok(None),
    };
    let Some(correlation) = event.get("client_event_id") else {
        return Ok(Some(LiveEvent::Ignored));
    };
    let client_event_id = correlation
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 128)
        .ok_or(LiveError::InvalidEvent)?
        .to_owned();
    Ok(Some(LiveEvent::Acknowledged {
        client_event_id,
        kind,
    }))
}
