//! Agent live node factories. Agent 节点复用 terminal-backed runtime。

use crate::nodes::term::TermLive;
use canvas::AgentProfile;
use terminal::{PtyCommand, PtyError, TerminalSessionId};

pub fn spawn_agent_live(
    id: TerminalSessionId,
    profile: AgentProfile,
    cols: u16,
    rows: u16,
) -> Result<TermLive, PtyError> {
    TermLive::spawn_command(id, agent_command(profile), cols, rows)
}

pub(crate) fn agent_command(profile: AgentProfile) -> PtyCommand {
    match profile {
        AgentProfile::Claude => {
            PtyCommand::new("/bin/zsh").args(["-ic", "exec claude --dangerously-skip-permissions"])
        }
        AgentProfile::Codex => PtyCommand::new("/bin/zsh").args([
            "-ic",
            "exec codex --dangerously-bypass-approvals-and-sandbox",
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Agent profile 映射到对应 CLI 启动命令。
    ///
    /// 数据构造（含关键数值推导）：
    ///   Claude profile → /bin/zsh -ic "exec claude --dangerously-skip-permissions"
    ///   Codex profile  → /bin/zsh -ic "exec codex --dangerously-bypass-approvals-and-sandbox"
    ///   两个命令都使用 3 段 argv：program + "-ic" + exec 命令
    ///
    /// 执行过程：
    ///   1. agent_command(Claude)
    ///   2. agent_command(Codex)
    ///   3. 检查 program 与 args
    ///
    /// 预期结果：
    ///   - 正断言：两个 profile 都通过 /bin/zsh -ic 启动
    ///   - 正断言：Claude/Codex 分别包含对应 CLI 名称
    ///   - 负断言：Claude 命令不应误用 codex，Codex 命令不应误用 claude
    #[test]
    fn test_agent_command_maps_profile_to_expected_cli() {
        let claude = agent_command(AgentProfile::Claude);
        let codex = agent_command(AgentProfile::Codex);

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
    }
}
