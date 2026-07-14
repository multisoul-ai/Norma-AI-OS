use norma::cli::parse_command_from;
use norma::protocol::{AgentProfile, CardCommand, CardKind, Placement, PreviewType};
use std::path::PathBuf;

/// Norma CLI：`card create terminal` 应生成创建 Terminal 的 CardCommand。
///
/// 数据构造（含关键数值的推导过程）：
///   argv = ["norma", "card", "create", "terminal"]
///   shell 未传入，因此 shell=None，app 使用默认 shell
///   placement 未传入，因此 placement=VisibleArea
///
/// 执行过程（逐步说明系统如何处理）：
///   1. parse_command_from(argv)
///   2. 读取生成的 CardCommand
///
/// 预期结果：
///   - 正断言：生成 Create Terminal
///   - 正断言：placement 是 VisibleArea
///   - 负断言：不应生成 Agent 或 Preview
#[test]
fn test_parse_create_terminal_command() {
    let command = parse_command_from(["norma", "card", "create", "terminal"])
        .expect("terminal create CLI 应解析成功");

    assert_eq!(
        command,
        CardCommand::Create {
            kind: CardKind::Terminal { shell: None },
            placement: Placement::VisibleArea,
        },
        "card create terminal 应生成默认 shell 的 Terminal 创建命令"
    );
    assert_ne!(
        command,
        CardCommand::Create {
            kind: CardKind::Agent {
                profile: AgentProfile::Codex,
            },
            placement: Placement::VisibleArea,
        },
        "terminal create 不应误生成 Codex Agent"
    );
}

/// Norma CLI：`card create agent --profile codex` 应生成 Codex Agent 创建命令。
///
/// 数据构造（含关键数值的推导过程）：
///   argv profile = "codex"（小写命令行输入）
///   AgentProfile::Codex 是协议枚举，app 再映射为 canvas::AgentProfile
///
/// 执行过程（逐步说明系统如何处理）：
///   1. parse_command_from(argv)
///   2. 检查 CardKind::Agent profile
///
/// 预期结果：
///   - 正断言：profile 被解析为 Codex
///   - 负断言：不应解析为 Claude
#[test]
fn test_parse_create_codex_agent_command() {
    let command = parse_command_from(["norma", "card", "create", "agent", "--profile", "codex"])
        .expect("agent create CLI 应解析成功");

    assert_eq!(
        command,
        CardCommand::Create {
            kind: CardKind::Agent {
                profile: AgentProfile::Codex,
            },
            placement: Placement::VisibleArea,
        },
        "card create agent --profile codex 应生成 Codex Agent"
    );
    assert_ne!(
        command,
        CardCommand::Create {
            kind: CardKind::Agent {
                profile: AgentProfile::Claude,
            },
            placement: Placement::VisibleArea,
        },
        "codex profile 不应被误解析为 Claude"
    );
}

/// Norma CLI：`card create agent --profile cursor` 应生成 Cursor Agent 创建命令。
///
/// 数据构造（含关键数值的推导过程）：
///   argv profile = "cursor"（小写命令行输入）；创建次数 = 1。
///   AgentProfile::Cursor 是协议枚举，app 会将它映射为 canvas::AgentProfile。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. `parse_command_from` 解析 CLI 子命令和 `--profile` 值。
///   2. 读取生成的 `CardKind::Agent`，检查 profile 枚举值。
///
/// 预期结果：
///   - 断言 A：profile 精确解析为 Cursor，CLI 可创建 Cursor Agent。
///   - 断言 B：profile 不得误解析为 Codex，避免启动错误的 Agent CLI。
#[test]
fn test_parse_create_cursor_agent_command() {
    let command = parse_command_from(["norma", "card", "create", "agent", "--profile", "cursor"])
        .expect("Cursor agent create CLI 应解析成功");

    assert_eq!(
        command,
        CardCommand::Create {
            kind: CardKind::Agent {
                profile: AgentProfile::Cursor,
            },
            placement: Placement::VisibleArea,
        },
        "card create agent --profile cursor 应生成 Cursor Agent"
    );
    assert_ne!(
        command,
        CardCommand::Create {
            kind: CardKind::Agent {
                profile: AgentProfile::Codex,
            },
            placement: Placement::VisibleArea,
        },
        "cursor profile 不应被误解析为 Codex"
    );
}

/// Norma CLI：`card create agent --profile opencode` 应生成 OpenCode Agent 创建命令。
///
/// 数据构造（含关键数值的推导过程）：
///   argv profile = "opencode"（小写命令行输入）
///   AgentProfile::OpenCode 是协议枚举，app 再映射为 canvas::AgentProfile
///
/// 执行过程（逐步说明系统如何处理）：
///   1. parse_command_from(argv)
///   2. 检查 CardKind::Agent profile
///
/// 预期结果：
///   - 正断言：profile 被解析为 OpenCode
///   - 负断言：不应解析为 Claude 或 Codex
#[test]
fn test_parse_create_opencode_agent_command() {
    let command = parse_command_from(["norma", "card", "create", "agent", "--profile", "opencode"])
        .expect("OpenCode agent create CLI 应解析成功");

    assert_eq!(
        command,
        CardCommand::Create {
            kind: CardKind::Agent {
                profile: AgentProfile::OpenCode,
            },
            placement: Placement::VisibleArea,
        },
        "card create agent --profile opencode 应生成 OpenCode Agent"
    );
    assert_ne!(
        command,
        CardCommand::Create {
            kind: CardKind::Agent {
                profile: AgentProfile::Claude,
            },
            placement: Placement::VisibleArea,
        },
        "opencode profile 不应被误解析为 Claude"
    );
    assert_ne!(
        command,
        CardCommand::Create {
            kind: CardKind::Agent {
                profile: AgentProfile::Codex,
            },
            placement: Placement::VisibleArea,
        },
        "opencode profile 不应被误解析为 Codex"
    );
}

/// Norma CLI：`card create preview PATH --type markdown` 应生成 Preview 创建命令。
///
/// 数据构造（含关键数值的推导过程）：
///   path = "./README.md"（CLI 先保留用户输入路径，app 决定如何 canonicalize）
///   --type markdown → PreviewType::Markdown
///   placement 默认 VisibleArea
///
/// 执行过程（逐步说明系统如何处理）：
///   1. parse_command_from(argv)
///   2. 检查 CardKind::Preview path/type
///
/// 预期结果：
///   - 正断言：path 和 Markdown 类型保留
///   - 负断言：不应生成 Terminal 命令
#[test]
fn test_parse_create_preview_command() {
    let command = parse_command_from([
        "norma",
        "card",
        "create",
        "preview",
        "./README.md",
        "--type",
        "markdown",
    ])
    .expect("preview create CLI 应解析成功");

    assert_eq!(
        command,
        CardCommand::Create {
            kind: CardKind::Preview {
                path: PathBuf::from("./README.md"),
                explicit_type: Some(PreviewType::Markdown),
            },
            placement: Placement::VisibleArea,
        },
        "card create preview 应保留路径与显式 Markdown 类型"
    );
    assert_ne!(
        command,
        CardCommand::Create {
            kind: CardKind::Terminal { shell: None },
            placement: Placement::VisibleArea,
        },
        "preview create 不应误生成 Terminal 命令"
    );
}

/// Norma CLI：`card send NODE TEXT --submit` 应生成补交命令，具体补换行由 app 执行。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 7（目标卡片 id）
///   text = "cargo test -p runtime"（不含换行）
///   submit = true（app 执行时补一个换行）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. parse_command_from(argv)
///   2. 检查 SendText node/text/submit
///
/// 预期结果：
///   - 正断言：node_id=7、submit=true
///   - 负断言：CLI parse 阶段不应提前追加换行
#[test]
fn test_parse_send_submit_command() {
    let command = parse_command_from([
        "norma",
        "card",
        "send",
        "7",
        "cargo test -p runtime",
        "--submit",
    ])
    .expect("send CLI 应解析成功");

    assert_eq!(
        command,
        CardCommand::SendText {
            node_id: 7,
            text: "cargo test -p runtime".to_string(),
            submit: true,
        },
        "card send 应生成目标 node_id=7 的 SendText 命令"
    );
    assert_ne!(
        command,
        CardCommand::SendText {
            node_id: 7,
            text: "cargo test -p runtime\n".to_string(),
            submit: true,
        },
        "CLI parse 阶段不应提前追加换行，避免重复提交"
    );
}
