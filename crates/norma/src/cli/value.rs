use super::{AgentProfileValue, PreviewTypeValue};
use crate::protocol::{AgentProfile, PreviewType};

impl From<AgentProfileValue> for AgentProfile {
    fn from(value: AgentProfileValue) -> Self {
        match value {
            AgentProfileValue::Claude => AgentProfile::Claude,
            AgentProfileValue::Codex => AgentProfile::Codex,
            AgentProfileValue::Cursor => AgentProfile::Cursor,
            AgentProfileValue::Grok => AgentProfile::Grok,
            AgentProfileValue::OpenCode => AgentProfile::OpenCode,
        }
    }
}

impl From<PreviewTypeValue> for PreviewType {
    fn from(value: PreviewTypeValue) -> Self {
        match value {
            PreviewTypeValue::Markdown => PreviewType::Markdown,
            PreviewTypeValue::Text => PreviewType::Text,
            PreviewTypeValue::Image => PreviewType::Image,
        }
    }
}
