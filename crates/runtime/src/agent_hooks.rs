//! Agent hook launch environment for terminal-backed Agent nodes.

use crate::runtime::Runtime;
use canvas::{AgentProfile, NodeId};
use terminal::PtyCommand;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentHookRuntimeConfig {
    pub endpoint: String,
    pub token: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentHookEnv {
    pub(crate) node_id: NodeId,
    pub(crate) profile: AgentProfile,
    pub(crate) endpoint: String,
    pub(crate) token: String,
}

impl AgentHookEnv {
    pub(crate) fn apply_to_command(&self, command: PtyCommand) -> PtyCommand {
        command
            .env("MULTISOUL_NODE_ID", self.node_id.to_string())
            .env("MULTISOUL_AGENT_PROFILE", agent_profile_env(self.profile))
            .env("MULTISOUL_HOOK_ENDPOINT", self.endpoint.clone())
            .env("MULTISOUL_HOOK_TOKEN", self.token.clone())
    }
}

impl Runtime {
    pub fn set_agent_hook_config(&mut self, config: Option<AgentHookRuntimeConfig>) {
        self.agent_hook_config = config;
    }

    pub fn agent_hook_config(&self) -> Option<&AgentHookRuntimeConfig> {
        self.agent_hook_config.as_ref()
    }

    pub(crate) fn agent_hook_env(
        &self,
        node_id: NodeId,
        profile: AgentProfile,
    ) -> Option<AgentHookEnv> {
        if profile == AgentProfile::OpenCode {
            return None;
        }
        self.agent_hook_config.as_ref().map(|config| AgentHookEnv {
            node_id,
            profile,
            endpoint: config.endpoint.clone(),
            token: config.token.clone(),
        })
    }
}

fn agent_profile_env(profile: AgentProfile) -> &'static str {
    match profile {
        AgentProfile::Claude => "claude",
        AgentProfile::Codex => "codex",
        AgentProfile::OpenCode => "opencode",
    }
}
