use super::*;
use canvas::AgentProfile;
use storage::{NodeSession, NodeSessionKind};

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

/// Codex agent restore 应使用保存的 session_id 构造交互式 resume 命令。
///
/// 数据构造（含关键数值推导）：
///   profile = Codex
///   session_id = "019f1bdb-40df-7813-8057-60f429c38986"
///   workspace_root = "/tmp/multisoul-codex-workspace"
///   NodeSession.kind = Agent，因此应允许 profile-specific resume
///
/// 执行过程：
///   1. 构造带 Codex session_id 的 NodeSession
///   2. 调用 build_agent_restore_command_for_test(session, Codex, None, workspace_root)
///   3. 检查 args 中 Codex 使用 `resume <session_id>`，并包含 workspace `--cd`
///
/// 预期结果：
///   - 正断言：命令包含 `codex`、`resume`、session_id、`--cd`
///   - 负断言：Codex restore 不应回退到 fresh command（即缺少 resume）
#[test]
fn test_restore_codex_agent_uses_saved_session_id_for_resume() {
    let workspace_root = std::path::Path::new("/tmp/multisoul-codex-workspace");
    let session = NodeSession {
        node_id: 9,
        kind: NodeSessionKind::Agent,
        cwd: None,
        session_id: Some("019f1bdb-40df-7813-8057-60f429c38986".into()),
    };

    let cmd = crate::session_restore::build_agent_restore_command_for_test(
        &session,
        AgentProfile::Codex,
        None,
        Some(workspace_root),
    );
    let args_str = cmd.args.join(" ");

    assert!(
        args_str.contains("codex"),
        "Codex restore command should invoke codex, actual args: {args_str}"
    );
    assert!(
        args_str.contains("resume"),
        "Codex restore command should include resume subcommand, actual args: {args_str}"
    );
    assert!(
        args_str.contains("019f1bdb-40df-7813-8057-60f429c38986"),
        "Codex restore command should include the saved session id, actual args: {args_str}"
    );
    assert!(
        args_str.contains("--cd '/tmp/multisoul-codex-workspace'"),
        "Codex restore command should keep cwd filtering with --cd, actual args: {args_str}"
    );
    assert!(
        !args_str.ends_with("codex --dangerously-bypass-approvals-and-sandbox"),
        "Codex restore must not fall back to a fresh agent command"
    );
}

/// Claude agent restore 在旧库 session_id 为空时，应允许通过 workspace 发现到的 session_id 兜底恢复。
///
/// 数据构造（含关键数值推导）：
///   profile = Claude
///   NodeSession.kind = Agent
///   NodeSession.session_id = None（旧库或之前路径编码错误导致没有写入）
///   discovered_session_id = "real-claude-session"
///   workspace_root = "/tmp/multisoul-claude-workspace"
///
/// 执行过程：
///   1. 构造 session_id=None 的 Claude NodeSession
///   2. 调用 resolve_agent_restore_session_id_for_test，模拟 workspace 发现到 real-claude-session
///   3. 检查 resolver 返回 discovered session id，而不是 None
///
/// 预期结果：
///   - 正断言：返回 real-claude-session，恢复阶段可生成 `claude --resume`
///   - 负断言：不返回 None，否则已有空 node_sessions 的 workspace 仍无法恢复 Claude 会话
#[test]
fn test_restore_claude_agent_falls_back_to_discovered_session_id() {
    let workspace_root = std::path::Path::new("/tmp/multisoul-claude-workspace");
    let session = NodeSession {
        node_id: 10,
        kind: NodeSessionKind::Agent,
        cwd: None,
        session_id: None,
    };

    let resolved = crate::session_restore::resolve_agent_restore_session_id_for_test(
        Some(&session),
        AgentProfile::Claude,
        Some(workspace_root),
        Some("real-claude-session"),
    );

    assert_eq!(
        resolved.as_deref(),
        Some("real-claude-session"),
        "Claude restore 应在保存值缺失时使用 workspace 发现到的 session id"
    );
    assert!(
        resolved.is_some(),
        "旧库 session_id=None 但 Claude jsonl 存在时不应继续返回 None"
    );
}
