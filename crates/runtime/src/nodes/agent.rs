//! Agent live node factories. Agent 节点复用 terminal-backed runtime。

use crate::agent_hooks::AgentHookEnv;
use crate::nodes::term::TermLive;
use canvas::AgentProfile;
use std::path::Path;
use terminal::{PtyCommand, PtyError, TerminalSessionId};

pub(crate) fn spawn_agent_live(
    id: TerminalSessionId,
    profile: AgentProfile,
    cwd: Option<&Path>,
    hook_env: Option<&AgentHookEnv>,
    cols: u16,
    rows: u16,
) -> Result<TermLive, PtyError> {
    TermLive::spawn_command(
        id,
        agent_command(profile, hook_env).with_cwd(cwd),
        cols,
        rows,
    )
}

pub(crate) fn agent_command(profile: AgentProfile, hook_env: Option<&AgentHookEnv>) -> PtyCommand {
    let command = match profile {
        AgentProfile::Claude => {
            PtyCommand::new("/bin/zsh").args(["-ic", "exec claude --dangerously-skip-permissions"])
        }
        AgentProfile::Codex => PtyCommand::new("/bin/zsh").args([
            "-ic",
            "exec codex --dangerously-bypass-approvals-and-sandbox",
        ]),
    };
    match hook_env {
        Some(hook_env) => hook_env.apply_to_command(command),
        None => command,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Agent profile 映射到对应 CLI 启动命令，未传 hook env 时不注入 Multisoul 环境变量。
    ///
    /// 数据构造（含关键数值推导）：
    ///   Claude profile → /bin/zsh -ic "exec claude --dangerously-skip-permissions"
    ///   Codex profile  → /bin/zsh -ic "exec codex --dangerously-bypass-approvals-and-sandbox"
    ///   两个命令都使用 3 段 argv：program + "-ic" + exec 命令
    ///
    /// 执行过程：
    ///   1. agent_command(Claude, None)
    ///   2. agent_command(Codex, None)
    ///   3. 检查 program 与 args
    ///
    /// 预期结果：
    ///   - 正断言：两个 profile 都通过 /bin/zsh -ic 启动
    ///   - 正断言：Claude/Codex 分别包含对应 CLI 名称
    ///   - 负断言：Claude/Codex 命令不应带 MULTISOUL_NODE_ID
    #[test]
    fn test_agent_command_maps_profile_to_expected_cli() {
        let claude = agent_command(AgentProfile::Claude, None);
        let codex = agent_command(AgentProfile::Codex, None);

        assert_eq!(
            claude.program, "/bin/zsh",
            "Claude Agent 应通过 /bin/zsh 启动交互 shell 命令"
        );
        assert_eq!(
            codex.program, "/bin/zsh",
            "Codex Agent 应通过 /bin/zsh 启动交互 shell 命令"
        );
        assert_eq!(
            claude.args[0], "-ic",
            "Claude Agent 应使用 zsh -ic 执行 CLI"
        );
        assert_eq!(codex.args[0], "-ic", "Codex Agent 应使用 zsh -ic 执行 CLI");
        assert!(
            claude.args[1].contains("claude"),
            "Claude Agent 命令应包含 claude CLI"
        );
        assert!(
            codex.args[1].contains("codex"),
            "Codex Agent 命令应包含 codex CLI"
        );
        assert!(
            !claude.args[1].contains("codex"),
            "Claude Agent 命令不应误用 codex CLI"
        );
        assert!(
            !codex.args[1].contains("claude"),
            "Codex Agent 命令不应误用 claude CLI"
        );
        assert!(
            claude.env.iter().all(|(key, _)| key != "MULTISOUL_NODE_ID"),
            "未传 hook env 时 Claude 命令不应注入 MULTISOUL_NODE_ID"
        );
        assert!(
            codex.env.iter().all(|(key, _)| key != "MULTISOUL_NODE_ID"),
            "未传 hook env 时 Codex 命令不应注入 MULTISOUL_NODE_ID"
        );
    }

    /// Agent hook env 应按 node_id/profile/endpoint/token 注入 PtyCommand。
    ///
    /// 数据构造（含关键数值推导）：
    ///   node_id=42
    ///   profile=Codex → MULTISOUL_AGENT_PROFILE="codex"
    ///   endpoint="http://127.0.0.1:49152/agent-hooks"
    ///   token="tok_123"
    ///
    /// 执行过程：
    ///   1. 构造 AgentHookEnv
    ///   2. agent_command(Codex, Some(env))
    ///   3. 读取 command.env
    ///
    /// 预期结果：
    ///   - 正断言：四个 MULTISOUL_* 环境变量都存在且值精确
    ///   - 负断言：profile 不应误写为 claude
    #[test]
    fn test_agent_command_injects_multisoul_hook_environment() {
        let hook_env = AgentHookEnv {
            node_id: 42,
            profile: AgentProfile::Codex,
            endpoint: "http://127.0.0.1:49152/agent-hooks".to_string(),
            token: "tok_123".to_string(),
        };

        let command = agent_command(AgentProfile::Codex, Some(&hook_env));

        assert!(
            command
                .env
                .iter()
                .any(|(key, value)| key == "MULTISOUL_NODE_ID" && value == "42"),
            "hook env 应注入 MULTISOUL_NODE_ID=42，实际: {:?}",
            command.env
        );
        assert!(
            command
                .env
                .iter()
                .any(|(key, value)| key == "MULTISOUL_AGENT_PROFILE" && value == "codex"),
            "hook env 应注入 MULTISOUL_AGENT_PROFILE=codex，实际: {:?}",
            command.env
        );
        assert!(
            command
                .env
                .iter()
                .any(|(key, value)| key == "MULTISOUL_HOOK_ENDPOINT"
                    && value == "http://127.0.0.1:49152/agent-hooks"),
            "hook env 应注入 endpoint，实际: {:?}",
            command.env
        );
        assert!(
            command
                .env
                .iter()
                .any(|(key, value)| key == "MULTISOUL_HOOK_TOKEN" && value == "tok_123"),
            "hook env 应注入 token，实际: {:?}",
            command.env
        );
        assert!(
            command
                .env
                .iter()
                .all(|(key, value)| key != "MULTISOUL_AGENT_PROFILE" || value != "claude"),
            "Codex hook env 不应误写 profile=claude"
        );
    }
}
