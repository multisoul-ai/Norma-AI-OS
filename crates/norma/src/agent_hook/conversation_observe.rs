use super::AgentHookEvent;
use crate::protocol::{AgentProfile, NodeId};
use serde_json::Value;
use std::collections::HashSet;

/// Max Unicode characters retained for a single conversation message body (SPEC §5.3).
pub const MAX_CONVERSATION_MESSAGE_CHARS: usize = 1_200;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConversationObservationKind {
    UserMessage,
    CompleteAgentReply,
    UnavailableCompleteAgentReply,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationObservation {
    pub node_id: NodeId,
    pub profile: AgentProfile,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub kind: ConversationObservationKind,
    pub text: Option<String>,
    pub truncated: bool,
    pub event_fingerprint: String,
}

/// Idempotent merger for conversation observations within one session.
#[derive(Clone, Debug, Default)]
pub struct ConversationObservationState {
    seen_fingerprints: HashSet<String>,
    last_user: Option<ConversationObservation>,
    last_agent: Option<ConversationObservation>,
}

impl ConversationObservationState {
    pub fn apply(&mut self, observation: ConversationObservation) -> bool {
        if !self
            .seen_fingerprints
            .insert(observation.event_fingerprint.clone())
        {
            return false;
        }
        match observation.kind {
            ConversationObservationKind::UserMessage => {
                self.last_user = Some(observation);
            }
            ConversationObservationKind::CompleteAgentReply
            | ConversationObservationKind::UnavailableCompleteAgentReply => {
                self.last_agent = Some(observation);
            }
        }
        true
    }

    pub fn last_user(&self) -> Option<&ConversationObservation> {
        self.last_user.as_ref()
    }

    pub fn last_agent(&self) -> Option<&ConversationObservation> {
        self.last_agent.as_ref()
    }
}

/// Map a normalized hook event into a provider-neutral conversation observation.
///
/// Returns `None` for events that are neither user submits nor turn completions.
pub fn observe_conversation(event: &AgentHookEvent) -> Option<ConversationObservation> {
    match event.provider_event.as_str() {
        "UserPromptSubmit" => observe_user_message(event),
        "Stop" => observe_complete_agent_reply(event),
        _ => None,
    }
}

fn observe_user_message(event: &AgentHookEvent) -> Option<ConversationObservation> {
    let (text, truncated) =
        extract_bounded_text(&event.raw_json, USER_MESSAGE_KEYS).or_else(|| {
            // Kimi 的 prompt 是 [{type,text}] 数组，只有 Kimi 载荷会走到这里。
            super::kimi::kimi_prompt_text(&event.raw_json)
                .map(|text| truncate_chars(&text, MAX_CONVERSATION_MESSAGE_CHARS))
        })?;
    let fingerprint = fingerprint(event, "user", Some(text.as_str()));
    Some(ConversationObservation {
        node_id: event.node_id,
        profile: event.profile,
        session_id: event.session_id.clone(),
        turn_id: event.turn_id.clone(),
        kind: ConversationObservationKind::UserMessage,
        text: Some(text),
        truncated,
        event_fingerprint: fingerprint,
    })
}

fn observe_complete_agent_reply(event: &AgentHookEvent) -> Option<ConversationObservation> {
    let dedicated = extract_bounded_text(&event.raw_json, AGENT_REPLY_DEDICATED_KEYS);
    let generic = extract_bounded_text(&event.raw_json, AGENT_REPLY_GENERIC_KEYS);
    let extracted = match (dedicated, looks_like_tool_payload(&event.raw_json), generic) {
        (Some(value), _, _) => Some(value),
        (None, true, _) => None,
        (None, false, Some(value)) => Some(value),
        (None, false, None) => None,
    };
    match extracted {
        Some((text, truncated)) => {
            let fingerprint = fingerprint(event, "agent", Some(text.as_str()));
            Some(ConversationObservation {
                node_id: event.node_id,
                profile: event.profile,
                session_id: event.session_id.clone(),
                turn_id: event.turn_id.clone(),
                kind: ConversationObservationKind::CompleteAgentReply,
                text: Some(text),
                truncated,
                event_fingerprint: fingerprint,
            })
        }
        None => Some(unavailable(event)),
    }
}

fn unavailable(event: &AgentHookEvent) -> ConversationObservation {
    ConversationObservation {
        node_id: event.node_id,
        profile: event.profile,
        session_id: event.session_id.clone(),
        turn_id: event.turn_id.clone(),
        kind: ConversationObservationKind::UnavailableCompleteAgentReply,
        text: None,
        truncated: false,
        event_fingerprint: fingerprint(event, "agent-unavailable", None),
    }
}

const USER_MESSAGE_KEYS: &[&str] = &["prompt", "message", "user_message", "userMessage", "text"];

const AGENT_REPLY_DEDICATED_KEYS: &[&str] = &[
    "last_assistant_message",
    "lastAssistantMessage",
    "final_response",
    "finalResponse",
    "assistant_message",
    "assistantMessage",
    "response",
    "last_message",
    "lastMessage",
];

const AGENT_REPLY_GENERIC_KEYS: &[&str] = &["text"];

fn extract_bounded_text(raw: &Value, keys: &[&str]) -> Option<(String, bool)> {
    let text = keys.iter().find_map(|key| string_field(raw, key))?;
    Some(truncate_chars(&text, MAX_CONVERSATION_MESSAGE_CHARS))
}

fn looks_like_tool_payload(raw: &Value) -> bool {
    raw.get("tool_result").is_some()
        || raw.get("toolResult").is_some()
        || raw.get("tool_output").is_some()
        || raw.get("toolOutput").is_some()
        || raw
            .get("role")
            .and_then(Value::as_str)
            .is_some_and(|role| role.eq_ignore_ascii_case("tool"))
}

fn string_field(raw: &Value, field: &str) -> Option<String> {
    raw.get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn truncate_chars(value: &str, max_chars: usize) -> (String, bool) {
    let mut characters = value.chars();
    let prefix = characters.by_ref().take(max_chars).collect::<String>();
    if characters.next().is_some() {
        (prefix, true)
    } else {
        (prefix, false)
    }
}

fn fingerprint(event: &AgentHookEvent, kind: &str, text: Option<&str>) -> String {
    format!(
        "{}:{}:{}:{}:{}:{}",
        event.node_id,
        profile_as_str(event.profile),
        event.session_id.as_deref().unwrap_or(""),
        event.turn_id.as_deref().unwrap_or(""),
        kind,
        text.unwrap_or("")
    )
}

fn profile_as_str(profile: AgentProfile) -> &'static str {
    match profile {
        AgentProfile::Claude => "claude",
        AgentProfile::Codex => "codex",
        AgentProfile::Cursor => "cursor",
        AgentProfile::Grok => "grok",
        AgentProfile::OpenCode => "opencode",
        AgentProfile::Kimi => "kimi",
        AgentProfile::ZCode => "zcode",
        AgentProfile::Pi => "pi",
        AgentProfile::Hermes => "hermes",
    }
}
