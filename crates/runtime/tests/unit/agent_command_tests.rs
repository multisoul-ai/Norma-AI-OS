use super::*;
use crate::{AgentTaskState, AgentTaskUpdate};
use canvas::AgentProfile;
use storage::{NodeSession, NodeSessionKind};

/// Grok Build Agent：Runtime 必须通过用户 login shell 启动官方 `grok` TUI。
///
/// 数据构造（含关键数值的推导过程）：
///   shell argv 段数 = 2：`-lic` + `exec grok`
///   `-lic` = login(`l`) + interactive(`i`) + command(`c`)，使 GUI 进程读取用户 PATH
///   权限参数数量 = 0：一期保留 Grok 官方默认 `ask`，不自动附加 `--always-approve`
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用 `Runtime::agent_command(AgentProfile::Grok)`
///   2. Runtime 构造 `/bin/zsh -lic` 命令
///   3. zsh 以 `exec grok` 替换自身并进入 Grok Build TUI
///
/// 预期结果：
///   - 正断言：命令精确为 `/bin/zsh -lic "exec grok"`
///   - 正断言：命令包含官方 CLI 名称 `grok`
///   - 负断言：不得默认跳过 Grok 权限确认
#[test]
fn grok_agent_command_starts_official_tui_without_bypassing_approvals() {
    let grok = Runtime::agent_command(AgentProfile::Grok);

    assert_eq!(
        grok.program, "/bin/zsh",
        "Grok Agent 应通过 /bin/zsh 启动以继承用户 shell 环境"
    );
    assert_eq!(
        grok.args,
        vec!["-lic", "exec grok"],
        "Grok Agent args 必须精确启动官方 grok TUI"
    );
    assert!(
        grok.args.iter().any(|arg| arg.contains("grok")),
        "Grok Agent 命令必须包含官方 grok CLI 名称"
    );
    assert!(
        grok.args
            .iter()
            .all(|arg| !arg.contains("always-approve") && !arg.contains("yolo")),
        "Grok Agent 一期不得默认绕过官方权限确认"
    );
}

/// Grok Build 会话恢复：已保存的 hook session_id 必须传给官方 `--resume` 参数。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 21
///   session_id = `grok-session-21`，由 SessionStart hook 缓存到项目 NodeSession
///   workspace_root = `/tmp/norma-grok-workspace`
///   恢复候选数量 = 2：保存值 1 + 后台发现值 1；保存值优先级更高
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 profile=Grok 且带保存 session_id 的 Agent NodeSession
///   2. 调用公开测试 seam 构造恢复命令，并传入冲突的发现值
///   3. Runtime 选择保存值并生成 `grok --resume <ID>`
///
/// 预期结果：
///   - 正断言：命令包含 `grok --resume 'grok-session-21'`
///   - 正断言：cwd 保持为当前 workspace
///   - 负断言：不得使用冲突的发现值或退化成 fresh `exec grok`
#[test]
fn grok_agent_restore_uses_saved_hook_session_id() {
    let workspace_root = std::path::Path::new("/tmp/norma-grok-workspace");
    let session = NodeSession {
        node_id: 21,
        kind: NodeSessionKind::Agent,
        cwd: None,
        session_id: Some("grok-session-21".into()),
    };

    let command = crate::session_restore::build_agent_restore_command_with_discovery_for_test(
        &session,
        AgentProfile::Grok,
        None,
        Some(workspace_root),
        Some("grok-discovered-wrong-session"),
    );
    let args = command.args.join(" ");

    assert!(
        args.contains("grok --resume 'grok-session-21'"),
        "Grok restore 必须使用保存的 hook session_id，实际 args: {args}"
    );
    assert_eq!(
        command.cwd.as_deref(),
        Some(workspace_root),
        "Grok restore 必须在当前 workspace root 中启动"
    );
    assert!(
        !args.contains("grok-discovered-wrong-session"),
        "Grok restore 不得用后台发现值覆盖卡片保存的 session_id，实际 args: {args}"
    );
    assert_ne!(
        args, "-lic exec grok",
        "存在保存 session_id 时 Grok restore 不得退化为 fresh TUI 启动"
    );
}

/// Claude/Codex/OpenCode Agent profile 映射到精确的启动命令。
///
/// 数据构造（含关键数值推导）：
///   Claude profile → /bin/zsh -lic "exec claude --dangerously-skip-permissions"
///   Codex profile  → /bin/zsh -lic "exec codex -c disable_paste_burst=true --dangerously-bypass-approvals-and-sandbox"
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
        vec!["-lic", "exec claude --dangerously-skip-permissions"],
        "Claude Agent args 必须加载 login profile 并精确匹配计划中的高权限命令"
    );
    assert_eq!(
        codex.args,
        vec![
            "-lic",
            "exec codex -c disable_paste_burst=true --dangerously-bypass-approvals-and-sandbox"
        ],
        "Codex Agent args 必须加载 login profile、精确匹配高权限命令，并关闭会吞掉自动 Enter 的 paste-burst 启发式"
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
///   discovered_session_id = "019f-latest-but-wrong"（同工作空间最新文件，不属于此卡片）
///   workspace_root = "/tmp/multisoul-codex-workspace"
///   NodeSession.kind = Agent，因此应允许 profile-specific resume
///
/// 执行过程：
///   1. 构造带 Codex session_id 的 NodeSession
///   2. 调用 Agent 恢复命令构造边界，同时传入冲突的后台发现结果
///   3. 检查 args 中 Codex 使用持久化的 `resume <session_id>`，并包含 workspace `--cd`
///
/// 预期结果：
///   - 正断言：命令包含 `codex`、`resume`、session_id、`--cd`
///   - 负断言：Codex restore 不应回退到 fresh command（即缺少 resume）
///   - 负断言：不得使用 discovered_session_id 覆盖卡片精确持久化的 session_id
#[test]
fn test_restore_codex_agent_uses_saved_session_id_for_resume() {
    let workspace_root = std::path::Path::new("/tmp/multisoul-codex-workspace");
    let session = NodeSession {
        node_id: 9,
        kind: NodeSessionKind::Agent,
        cwd: None,
        session_id: Some("019f1bdb-40df-7813-8057-60f429c38986".into()),
    };

    let cmd = crate::session_restore::build_agent_restore_command_with_discovery_for_test(
        &session,
        AgentProfile::Codex,
        None,
        Some(workspace_root),
        Some("019f-latest-but-wrong"),
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
    assert!(
        !args_str.contains("019f-latest-but-wrong"),
        "Codex restore 必须优先使用该卡片持久化 session_id，不得误绑后台扫描到的其他会话，实际: {args_str}"
    );
}

/// T-title-1: 首次 UserPromptSubmit 将 task_title 写入 runtime，后续调用不覆盖。
///
/// 数据构造：
///   node_id = 1, profile = Claude
///   第一次 apply_agent_task_update task_title = Some("帮我重构")
///   第二次 apply_agent_task_update task_title = Some("现在再做别的")
///
/// 执行过程：
///   1. 两次 apply_agent_task_update，task_title 分别为 Some("帮我重构") 和 Some("现在再做别的")
///   2. 读取 runtime.agent_task_title(node_id)
///
/// 预期结果：
///   - 正断言：agent_task_title 为 "帮我重构"（首次写入，锁定）
///   - 负断言：不应为 "现在再做别的"（第二次不应覆盖）
#[test]
fn test_task_title_locked_on_first_set() {
    let mut runtime = Runtime::new();
    let node_id = runtime.document.add_node(
        canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
            profile: canvas::AgentProfile::Claude,
            alias: None,
            definition_id: None,
            mobile_control: false,
        }),
        canvas::CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );

    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id,
        profile: canvas::AgentProfile::Claude,
        provider_event: "UserPromptSubmit".to_string(),
        to: AgentTaskState::Thinking,
        task_title: Some("帮我重构".to_string()),
        session_id: None,
    });
    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id,
        profile: canvas::AgentProfile::Claude,
        provider_event: "UserPromptSubmit".to_string(),
        to: AgentTaskState::Thinking,
        task_title: Some("现在再做别的".to_string()),
        session_id: None,
    });

    assert_eq!(
        runtime.agent_task_title(node_id),
        Some("帮我重构".to_string()),
        "task_title 应在首次写入后锁定，不被第二次覆盖"
    );
    assert_ne!(
        runtime.agent_task_title(node_id),
        Some("现在再做别的".to_string()),
        "第二次 task_title 不应覆盖首次写入的值"
    );
}

/// T-title-2: task_title 为 None 时不写入，agent_task_title 返回 None。
///
/// 数据构造：
///   node_id = 1, profile = Codex
///   apply_agent_task_update task_title = None
///
/// 执行过程：
///   1. apply_agent_task_update task_title = None
///   2. 读取 runtime.agent_task_title(node_id)
///
/// 预期结果：
///   - 正断言：agent_task_title 返回 None
#[test]
fn test_task_title_none_does_not_write() {
    let mut runtime = Runtime::new();
    let node_id = runtime.document.add_node(
        canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
            profile: canvas::AgentProfile::Codex,
            alias: None,
            definition_id: None,
            mobile_control: false,
        }),
        canvas::CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );

    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id,
        profile: canvas::AgentProfile::Codex,
        provider_event: "PreToolUse".to_string(),
        to: AgentTaskState::RunningTool {
            tool_name: Some("bash".to_string()),
        },
        task_title: None,
        session_id: None,
    });

    assert_eq!(
        runtime.agent_task_title(node_id),
        None,
        "task_title=None 时不应写入任何值"
    );
}

/// T-title-3: 节点被移除后，task_title 也应清除。
///
/// 数据构造：
///   node_id = 1, profile = Claude, task_title = "test task"
///
/// 执行过程：
///   1. 写入 task_title
///   2. runtime.remove_node(node_id)
///   3. 读取 runtime.agent_task_title(node_id)
///
/// 预期结果：
///   - 正断言：agent_task_title 返回 None
#[test]
fn test_task_title_cleared_on_node_remove() {
    let mut runtime = Runtime::new();
    let node_id = runtime.document.add_node(
        canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
            profile: canvas::AgentProfile::Claude,
            alias: None,
            definition_id: None,
            mobile_control: false,
        }),
        canvas::CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );

    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id,
        profile: canvas::AgentProfile::Claude,
        provider_event: "UserPromptSubmit".to_string(),
        to: AgentTaskState::Thinking,
        task_title: Some("test task".to_string()),
        session_id: None,
    });

    runtime.remove_node(node_id);

    assert_eq!(
        runtime.agent_task_title(node_id),
        None,
        "节点移除后 task_title 应被清除"
    );
}

/// Claude Agent 恢复在持久化 session_id 缺失时，应使用后台发现的工作空间历史会话。
///
/// 数据构造（含关键数值推导）：
///   profile = Claude
///   NodeSession.kind = Agent
///   NodeSession.session_id = None（旧库未持久化 hook session id）
///   discovered_session_id = "claude-session-from-history"（后台扫描发现的历史会话）
///   workspace_root = "/tmp/multisoul-claude-workspace"
///
/// 执行过程：
///   1. 构造 session_id=None 的 Claude NodeSession
///   2. 后台会话发现返回 claude-session-from-history
///   3. 调用 Agent 恢复命令构造边界，合并持久化值与后台发现结果
///   4. 检查命令进入 `claude --resume claude-session-from-history`
///
/// 预期结果：
///   - 正断言：命令包含 `--resume` 与 claude-session-from-history
///   - 负断言：命令不得退回不带 `--resume` 的全新 Claude Session
#[test]
fn test_restore_claude_agent_without_saved_id_uses_background_discovery() {
    let workspace_root = std::path::Path::new("/tmp/multisoul-claude-workspace");
    let session = NodeSession {
        node_id: 10,
        kind: NodeSessionKind::Agent,
        cwd: None,
        session_id: None,
    };

    let command = crate::session_restore::build_agent_restore_command_with_discovery_for_test(
        &session,
        AgentProfile::Claude,
        None,
        Some(workspace_root),
        Some("claude-session-from-history"),
    );
    let args = command.args.join(" ");

    assert!(
        args.contains("--resume 'claude-session-from-history'"),
        "Claude restore 缺失持久化 session_id 时应使用后台发现结果生成 --resume 命令，实际: {args}"
    );
    assert!(
        args != "-ic exec claude --dangerously-skip-permissions",
        "后台已发现匹配工作空间的 Claude session 时不得退回全新会话命令，实际: {args}"
    );
}

/// T-title-4: 新 session 开始（Ready 状态）时，旧 task_title 被清除，允许新 prompt 写入。
///
/// 数据构造：
///   node_id = 1, profile = Claude
///   第一次 UserPromptSubmit task_title = "旧任务"
///   之后触发 SessionStart → Ready 状态
///   第二次 UserPromptSubmit task_title = "新任务"
///
/// 执行过程：
///   1. apply_agent_task_update task_title=Some("旧任务"), to=Thinking
///   2. apply_agent_task_update task_title=None, to=Ready（SessionStart）
///   3. apply_agent_task_update task_title=Some("新任务"), to=Thinking
///   4. 读取 runtime.agent_task_title(node_id)
///
/// 预期结果：
///   - 正断言：agent_task_title 为 "新任务"（新 session 的首次 prompt）
///   - 负断言：不应为 "旧任务"（旧 session 的标题已被 Ready 状态清除）
#[test]
fn test_task_title_cleared_on_new_session() {
    let mut runtime = Runtime::new();
    let node_id = runtime.document.add_node(
        canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
            profile: canvas::AgentProfile::Claude,
            alias: None,
            definition_id: None,
            mobile_control: false,
        }),
        canvas::CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );

    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id,
        profile: canvas::AgentProfile::Claude,
        provider_event: "UserPromptSubmit".to_string(),
        to: AgentTaskState::Thinking,
        task_title: Some("旧任务".to_string()),
        session_id: None,
    });
    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id,
        profile: canvas::AgentProfile::Claude,
        provider_event: "SessionStart".to_string(),
        to: AgentTaskState::Ready,
        task_title: None,
        session_id: None,
    });
    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id,
        profile: canvas::AgentProfile::Claude,
        provider_event: "UserPromptSubmit".to_string(),
        to: AgentTaskState::Thinking,
        task_title: Some("新任务".to_string()),
        session_id: None,
    });

    assert_eq!(
        runtime.agent_task_title(node_id),
        Some("新任务".to_string()),
        "新 session 的首次 prompt 应成为新 task_title"
    );
    assert_ne!(
        runtime.agent_task_title(node_id),
        Some("旧任务".to_string()),
        "旧 session 的 task_title 应在 Ready（SessionStart）时被清除"
    );
}
