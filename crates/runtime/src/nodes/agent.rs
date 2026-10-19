//! Agent live node factories. Agent 节点复用 terminal-backed runtime。

use crate::agent_hooks::AgentHookEnv;
use crate::nodes::term::{PtyFactory, TermLive};
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
    pty_factory: Option<&PtyFactory>,
) -> Result<TermLive, PtyError> {
    let command = agent_command(profile, hook_env).with_cwd(cwd);
    match pty_factory {
        Some(factory) => TermLive::spawn_with_factory(id, command, cols, rows, factory),
        None => TermLive::spawn_command(id, command, cols, rows),
    }
}

pub(crate) fn agent_command(profile: AgentProfile, hook_env: Option<&AgentHookEnv>) -> PtyCommand {
    let command = match profile {
        AgentProfile::Claude => {
            PtyCommand::new("/bin/zsh").args(["-lic", "exec claude --dangerously-skip-permissions"])
        }
        AgentProfile::Codex => {
            let mut exec_command =
                "exec codex -c disable_paste_burst=true --dangerously-bypass-approvals-and-sandbox"
                    .to_string();
            if hook_env.is_some() {
                // Codex 要求每个 hook 命令先经交互式信任确认才会执行；Multisoul 的
                // hook bridge 命令是非交互启动的，永远没机会走那个确认流程，所以
                // 只有在确实注入了 Multisoul hook env 时才带上这个标志去跳过信任门禁。
                exec_command.push_str(" --dangerously-bypass-hook-trust");
            }
            PtyCommand::new("/bin/zsh").arg("-lic").arg(exec_command)
        }
        AgentProfile::Cursor => PtyCommand::new("/bin/zsh").args(["-lic", "exec cursor-agent"]),
        AgentProfile::Grok => PtyCommand::new("/bin/zsh").args(["-lic", "exec grok"]),
        AgentProfile::OpenCode => {
            PtyCommand::new("/bin/zsh").args(["-lic", "exec \"${OPENCODE_BIN:-opencode}\""])
        }
        AgentProfile::InfCode => PtyCommand::new("/bin/zsh").args([
            "-lic",
            // InfCode `--yolo`：本进程内自动批准 TUI 会话及其递归 Task 后代的权限请求。
            "exec \"${INFCODE_BIN:-infcode}\" --yolo",
        ]),
        // 骨架占位：只使用已确认的命令名，不带任何跳过审批参数；各 CLI 负责人实测后补充。
        AgentProfile::Kimi => PtyCommand::new("/bin/zsh").args(["-lic", "exec kimi"]),
        AgentProfile::ZCode => PtyCommand::new("/bin/zsh")
            .args(["-lic", &crate::zcode_launch::zcode_launch_script("")]),
        AgentProfile::Pi => {
            PtyCommand::new("/bin/zsh").args(["-lic", &crate::pi_launch::pi_launch_script("")])
        }
        AgentProfile::Hermes => PtyCommand::new("/bin/zsh").args([
            "-lic",
            &crate::agent_install_hint::with_install_hint(
                "hermes",
                crate::agent_install_hint::HERMES_INSTALL_MESSAGE,
                "exec hermes",
            ),
        ]),
    };
    match hook_env {
        Some(hook_env) => hook_env.apply_to_command(command),
        None => command,
    }
}

/// Mobile creation and resume use the same full-access policy as desktop native cards.
pub(crate) fn mobile_agent_command(
    profile: AgentProfile,
    session: Option<&str>,
    hook: Option<&AgentHookEnv>,
) -> Result<PtyCommand, PtyError> {
    let resume = session.map(crate::session_snapshot::shell_quote);
    let command = match profile {
        AgentProfile::Claude => format!("exec claude --dangerously-skip-permissions{}", resume.map(|id| format!(" --resume {id}")).unwrap_or_default()),
        AgentProfile::Codex => format!("exec codex -c disable_paste_burst=true -c check_for_update_on_startup=false --dangerously-bypass-approvals-and-sandbox{}{}", if hook.is_some() { " --dangerously-bypass-hook-trust" } else { "" }, resume.map(|id| format!(" resume {id}")).unwrap_or_default()),
        AgentProfile::Cursor => format!(
            "exec cursor-agent{}",
            resume
                .map(|id| format!(" --resume {id}"))
                .unwrap_or_default()
        ),
        _ => return Err(PtyError::Spawn("mobile Provider unavailable".into())),
    };
    let command = PtyCommand::new("/bin/zsh").args(["-lic", &command]);
    Ok(match hook {
        Some(hook) => hook.apply_to_command(command),
        None => command,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Agent profile 映射到对应 CLI 启动命令，未传 hook env 时不注入 Multisoul 环境变量。
    ///
    /// 数据构造（含关键数值推导）：
    ///   Claude profile → /bin/zsh -lic "exec claude --dangerously-skip-permissions"
    ///   Codex profile  → /bin/zsh -lic "exec codex -c disable_paste_burst=true --dangerously-bypass-approvals-and-sandbox"
    ///   Cursor profile → /bin/zsh -lic "exec cursor-agent"
    ///   三个命令都使用 3 段 argv：program + "-lic" + exec 命令
    ///
    /// 执行过程：
    ///   1. agent_command(Claude, None)
    ///   2. agent_command(Codex, None)
    ///   3. agent_command(Cursor, None)
    ///   4. 检查 program 与 args
    ///
    /// 预期结果：
    ///   - 正断言：三个 profile 都通过 /bin/zsh -lic 启动
    ///   - 正断言：Claude/Codex 分别包含对应 CLI 名称
    ///   - 负断言：Claude/Codex 命令不应带 MULTISOUL_NODE_ID
    #[test]
    fn test_agent_command_maps_profile_to_expected_cli() {
        let claude = agent_command(AgentProfile::Claude, None);
        let codex = agent_command(AgentProfile::Codex, None);
        let cursor = agent_command(AgentProfile::Cursor, None);

        assert_eq!(
            claude.program, "/bin/zsh",
            "Claude Agent 应通过 /bin/zsh 启动交互 shell 命令"
        );
        assert_eq!(
            codex.program, "/bin/zsh",
            "Codex Agent 应通过 /bin/zsh 启动交互 shell 命令"
        );
        assert_eq!(
            claude.args[0], "-lic",
            "Claude Agent 应使用 zsh -lic 加载 login profile 后执行 CLI"
        );
        assert_eq!(
            codex.args[0], "-lic",
            "Codex Agent 应使用 zsh -lic 加载 login profile 后执行 CLI"
        );
        assert_eq!(
            cursor.args[0], "-lic",
            "Cursor Agent 应使用 zsh -lic 加载 login profile 后执行 CLI"
        );
        assert!(
            claude.args[1].contains("claude"),
            "Claude Agent 命令应包含 claude CLI"
        );
        assert!(
            codex.args[1].contains("codex"),
            "Codex Agent 命令应包含 codex CLI"
        );
        assert!(
            codex.args[1].contains("-c disable_paste_burst=true"),
            "Codex Agent 必须关闭 paste-burst 启发式，否则自动输入后的 Enter 会先于正文落定"
        );
        assert!(
            cursor.args[1].contains("cursor-agent"),
            "Cursor Agent 命令应包含兼容的 cursor-agent CLI 入口"
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
            !cursor.args[1].contains("codex"),
            "Cursor Agent 命令不应误用 codex CLI"
        );
        assert!(
            claude.env.iter().all(|(key, _)| key != "MULTISOUL_NODE_ID"),
            "未传 hook env 时 Claude 命令不应注入 MULTISOUL_NODE_ID"
        );
        assert!(
            codex.env.iter().all(|(key, _)| key != "MULTISOUL_NODE_ID"),
            "未传 hook env 时 Codex 命令不应注入 MULTISOUL_NODE_ID"
        );
        assert!(
            cursor.env.iter().all(|(key, _)| key != "MULTISOUL_NODE_ID"),
            "未传 hook env 时 Cursor 命令不应注入 MULTISOUL_NODE_ID"
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
            workspace_id: "workspace-alpha".to_string(),
            workspace_route_id: "route-alpha-7".to_string(),
            norma_socket: "/tmp/multisoul-norma-dev.sock".to_string(),
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

    /// Codex 命令只在注入了 Multisoul hook env 时才带 `--dangerously-bypass-hook-trust`。
    ///
    /// 数据构造（含关键数值推导）：
    ///   Codex CLI（>=0.142.0）对每条 hooks.json 命令要求先经交互式信任确认才会执行；
    ///   Multisoul 的 hook bridge 命令以非交互方式启动，永远没有机会完成那次确认，
    ///   因此必须用 `--dangerously-bypass-hook-trust` 跳过门禁，否则 hook 永远不会真正运行。
    ///   该标志只在确实注入了 hook env 时才有意义，没有 hook env 时不应附带，
    ///   避免无谓放宽用户其他 hook（例如本机已安装的第三方 hook）的信任要求。
    ///
    /// 执行过程：
    ///   1. agent_command(Codex, None)
    ///   2. agent_command(Codex, Some(hook_env))
    ///   3. 检查 args[1] 是否包含该标志
    ///
    /// 预期结果：
    ///   - 正断言：带 hook env 时命令包含 `--dangerously-bypass-hook-trust`
    ///   - 负断言：不带 hook env 时命令不应包含该标志
    ///   - 负断言：Claude 命令永远不应包含该标志（Claude CLI 没有这个参数）
    #[test]
    fn test_codex_command_bypasses_hook_trust_only_with_hook_env() {
        let codex_without_hook_env = agent_command(AgentProfile::Codex, None);
        let hook_env = AgentHookEnv {
            node_id: 7,
            profile: AgentProfile::Codex,
            endpoint: "http://127.0.0.1:49153/agent-hooks".to_string(),
            token: "tok_456".to_string(),
            workspace_id: "workspace-alpha".to_string(),
            workspace_route_id: "route-alpha-7".to_string(),
            norma_socket: "/tmp/multisoul-norma-dev.sock".to_string(),
        };
        let codex_with_hook_env = agent_command(AgentProfile::Codex, Some(&hook_env));
        let claude_with_hook_env = agent_command(
            AgentProfile::Claude,
            Some(&AgentHookEnv {
                node_id: 7,
                profile: AgentProfile::Claude,
                endpoint: "http://127.0.0.1:49153/agent-hooks".to_string(),
                token: "tok_456".to_string(),
                workspace_id: "workspace-alpha".to_string(),
                workspace_route_id: "route-alpha-7".to_string(),
                norma_socket: "/tmp/multisoul-norma-dev.sock".to_string(),
            }),
        );

        assert!(
            codex_with_hook_env.args[1].contains("--dangerously-bypass-hook-trust"),
            "注入 hook env 时 Codex 命令应跳过 hook 信任门禁，实际: {:?}",
            codex_with_hook_env.args
        );
        assert!(
            !codex_without_hook_env.args[1].contains("--dangerously-bypass-hook-trust"),
            "未注入 hook env 时不应附带该标志，实际: {:?}",
            codex_without_hook_env.args
        );
        assert!(
            !claude_with_hook_env.args[1].contains("--dangerously-bypass-hook-trust"),
            "Claude CLI 没有该参数，命令中不应出现，实际: {:?}",
            claude_with_hook_env.args
        );
    }
}
