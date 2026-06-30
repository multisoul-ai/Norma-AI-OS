use super::*;
use canvas::AgentProfile;

/// Claude/Codex/OpenCode Agent profile 映射到精确的启动命令。
///
/// 数据构造（含关键数值推导）：
///   Claude profile → /bin/zsh -ic "exec claude --dangerously-skip-permissions"
///   Codex profile  → /bin/zsh -ic "exec codex --dangerously-bypass-approvals-and-sandbox"
///   OpenCode profile → /bin/zsh -lic "exec \"${OPENCODE_BIN:-opencode}\""
///   使用 -lic：login shell 先读 ~/.zprofile，再以交互模式读取 ~/.zshrc，exec 替换 shell 进程
///
/// 执行过程：
///   1. Runtime::agent_command(Claude)
///   2. Runtime::agent_command(Codex)
///   3. Runtime::agent_command(OpenCode)
///   4. 分别检查 program 与 args
///
/// 预期结果：
///   - 正断言：三个命令 program 都是 /bin/zsh
///   - 正断言：Claude/Codex/OpenCode args 精确匹配计划中的命令
///   - 负断言：OpenCode 命令不应包含 Claude/Codex 专属危险权限参数
#[test]
fn test_agent_command_specs_are_exact() {
    let claude = Runtime::agent_command(AgentProfile::Claude);
    let codex = Runtime::agent_command(AgentProfile::Codex);
    let opencode = Runtime::agent_command(AgentProfile::OpenCode);

    assert_eq!(
        claude.program, "/bin/zsh",
        "Claude Agent 应通过 /bin/zsh 启动以继承用户 shell 环境"
    );
    assert_eq!(
        codex.program, "/bin/zsh",
        "Codex Agent 应通过 /bin/zsh 启动以继承用户 shell 环境"
    );
    assert_eq!(
        opencode.program, "/bin/zsh",
        "OpenCode Agent 应通过 /bin/zsh 启动以继承用户 shell 环境"
    );
    assert_eq!(
        claude.args,
        vec!["-ic", "exec claude --dangerously-skip-permissions"],
        "Claude Agent args 必须精确匹配计划中的高权限命令"
    );
    assert_eq!(
        codex.args,
        vec![
            "-ic",
            "exec codex --dangerously-bypass-approvals-and-sandbox"
        ],
        "Codex Agent args 必须精确匹配计划中的高权限命令"
    );
    assert_eq!(
        opencode.args,
        vec!["-lic", "exec \"${OPENCODE_BIN:-opencode}\""],
        "OpenCode Agent args 应以 login interactive shell 启动 TUI，确保 GUI 环境读取 Homebrew PATH，并允许 OPENCODE_BIN 覆盖 binary"
    );
    assert!(
        !claude.args.iter().any(|arg| arg.contains("codex")),
        "Claude Agent 命令不应包含 codex 可执行名"
    );
    assert!(
        !codex.args.iter().any(|arg| arg.contains("claude")),
        "Codex Agent 命令不应包含 claude 可执行名"
    );
    assert!(
        !opencode.args.iter().any(|arg| arg.contains("dangerously")),
        "OpenCode TUI 命令不应继承 Claude/Codex 的危险权限参数"
    );
}
