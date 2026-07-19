use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum AgentProfile {
    Claude,
    Codex,
    Cursor,
    Grok,
    OpenCode,
}

impl AgentProfile {
    pub fn display_title(self) -> &'static str {
        match self {
            AgentProfile::Claude => "Claude",
            AgentProfile::Codex => "Codex",
            AgentProfile::Cursor => "Cursor",
            AgentProfile::Grok => "Grok",
            AgentProfile::OpenCode => "OpenCode",
        }
    }
}
