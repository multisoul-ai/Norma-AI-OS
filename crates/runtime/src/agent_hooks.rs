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

#[cfg(test)]
mod tests {
    use super::*;

    /// OpenCode Agent nodes should receive the same hook bridge environment as Claude/Codex.
    ///
    /// Data setup:
    ///   node_id = 77, representing one OpenCode Agent card.
    ///   profile = OpenCode, so MULTISOUL_AGENT_PROFILE should be "opencode".
    ///   endpoint/token come from AgentHookRuntimeConfig started by the app-local hook server.
    ///
    /// Execution:
    ///   1. Create Runtime and set AgentHookRuntimeConfig.
    ///   2. Request agent_hook_env(node_id, OpenCode).
    ///   3. Inspect the returned AgentHookEnv fields.
    ///
    /// Expected:
    ///   - Positive: env exists for OpenCode so the OpenCode plugin can POST status events.
    ///   - Positive: profile remains OpenCode and endpoint/token are preserved.
    ///   - Negative: OpenCode must not be treated as Codex.
    #[test]
    fn test_agent_hook_env_includes_opencode_profile() {
        let mut runtime = Runtime::new();
        runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
            endpoint: "http://127.0.0.1:49154/agent-hooks".to_string(),
            token: "tok_opencode".to_string(),
        }));

        let env = runtime
            .agent_hook_env(77, AgentProfile::OpenCode)
            .expect("OpenCode Agent should receive hook env");

        assert_eq!(
            env.node_id, 77,
            "OpenCode hook env should preserve the target node id"
        );
        assert_eq!(
            env.profile,
            AgentProfile::OpenCode,
            "OpenCode hook env should preserve profile=OpenCode"
        );
        assert_eq!(
            env.endpoint, "http://127.0.0.1:49154/agent-hooks",
            "OpenCode hook env should preserve the app-local endpoint"
        );
        assert_eq!(
            env.token, "tok_opencode",
            "OpenCode hook env should preserve the hook auth token"
        );
        assert_ne!(
            env.profile,
            AgentProfile::Codex,
            "OpenCode hook env must not be normalized to Codex"
        );
    }
}
