//! Cards 数字快捷键的 L1 验收：真实键事件必须创建对应的六类可运行卡片。

use canvas::{AgentProfile, CanvasNodeKind};
use gpui::TestAppContext;
use runtime::PtyFactory;
use std::sync::{Arc, Mutex};
use terminal::{PtyBackend, PtyCommand, TestPtyBackend};

use crate::harness::{echo_factory, enter_canvas, make_app, read_runtime};

/// E2E-CARDS-00：Agent 卡片必须通过 login interactive zsh 启动，以加载 Homebrew PATH。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent 快捷键数量 = 5（Cmd+2 至 Cmd+6）
///   捕获命令数量     = 5（每张 Agent 卡片各启动 1 个 PTY）
///   每条 zsh 参数    = 2（`-lic` + `exec <agent-cli>`）
///   `-lic`           = login(`l`) + interactive(`i`) + command(`c`)
///   负对照           = `-ic`（跳过 `.zprofile`，GUI PATH 下可能找不到 Homebrew zoxide）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 注入只记录 `PtyCommand` 的 PTY factory，并进入空 Canvas。
///   2. 模拟 Cmd+2..Cmd+6，经过真实快捷键、App action 与 Runtime Agent 创建链路。
///   3. PTY 边界依次捕获 Claude、Codex、Cursor、OpenCode、Grok 的启动命令。
///   4. 逐一检查每个 Agent 的首个 shell 参数。
///
/// 预期结果：
///   - 正断言：五种 Agent 的首个参数都精确为 `-lic`。
///   - 负断言：五种 Agent 都不得退回跳过 `.zprofile` 的 `-ic`。
#[test]
fn e2e_agent_cards_load_login_profile_before_interactive_zshrc() {
    let captured_commands = Arc::new(Mutex::new(Vec::<PtyCommand>::new()));
    let commands_for_factory = Arc::clone(&captured_commands);
    let factory: PtyFactory = Box::new(move |command, _cols, _rows, tx| {
        commands_for_factory
            .lock()
            .expect("Agent shell e2e 命令捕获锁不应中毒")
            .push(command);
        Ok(Box::new(TestPtyBackend::echo(tx)) as Box<dyn PtyBackend>)
    });
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, factory);
    let workspace = tempfile::TempDir::new().expect("Agent shell e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    cx.simulate_keystrokes("cmd-2 cmd-3 cmd-4 cmd-5 cmd-6");
    cx.run_until_parked();

    let commands = captured_commands
        .lock()
        .expect("Agent shell e2e 应读取已捕获命令");
    assert_eq!(
        commands.len(),
        5,
        "五个 Agent 快捷键应在 PTY 边界捕获恰好 5 条命令，实际为 {}",
        commands.len()
    );
    let claude = commands
        .first()
        .expect("第 1 条捕获命令应属于 Claude Agent");
    let codex = commands.get(1).expect("第 2 条捕获命令应属于 Codex Agent");
    let cursor = commands.get(2).expect("第 3 条捕获命令应属于 Cursor Agent");
    let opencode = commands
        .get(3)
        .expect("第 4 条捕获命令应属于 OpenCode Agent");
    let grok = commands.get(4).expect("第 5 条捕获命令应属于 Grok Agent");

    assert_eq!(
        claude.args.first().map(String::as_str),
        Some("-lic"),
        "Claude Agent 应先加载 login profile，再读取交互式 zshrc"
    );
    assert_ne!(
        claude.args.first().map(String::as_str),
        Some("-ic"),
        "Claude Agent 不得跳过包含 Homebrew PATH 的 .zprofile"
    );
    assert_eq!(
        codex.args.first().map(String::as_str),
        Some("-lic"),
        "Codex Agent 应先加载 login profile，再读取交互式 zshrc"
    );
    assert_ne!(
        codex.args.first().map(String::as_str),
        Some("-ic"),
        "Codex Agent 不得跳过包含 Homebrew PATH 的 .zprofile"
    );
    assert_eq!(
        cursor.args.first().map(String::as_str),
        Some("-lic"),
        "Cursor Agent 应先加载 login profile，再读取交互式 zshrc"
    );
    assert_ne!(
        cursor.args.first().map(String::as_str),
        Some("-ic"),
        "Cursor Agent 不得跳过包含 Homebrew PATH 的 .zprofile"
    );
    assert_eq!(
        opencode.args.first().map(String::as_str),
        Some("-lic"),
        "OpenCode Agent 应先加载 login profile，再读取交互式 zshrc"
    );
    assert_ne!(
        opencode.args.first().map(String::as_str),
        Some("-ic"),
        "OpenCode Agent 不得跳过包含 Homebrew PATH 的 .zprofile"
    );
    assert_eq!(
        grok.args.first().map(String::as_str),
        Some("-lic"),
        "Grok Agent 应先加载 login profile，再读取交互式 zshrc"
    );
    assert_ne!(
        grok.args.first().map(String::as_str),
        Some("-ic"),
        "Grok Agent 不得跳过包含 Homebrew PATH 的 .zprofile"
    );
}

/// E2E-CARDS-01：Cmd+1 到 Cmd+6 依次创建 Terminal、Claude、Codex、Cursor、OpenCode、Grok。
///
/// 数据构造（含关键数值的推导过程）：
///   初始节点数       = 0（fresh 临时 workspace）
///   数字快捷键次数   = 6（Cmd+1 至 Cmd+6 各一次）
///   预期 Terminal 数 = 1（Cmd+1）
///   预期 Agent 数    = 5（Cmd+2..Cmd+6）
///   预期总节点数     = 1 + 5 = 6
///   旧快捷键次数     = 1（Cmd+N，用于负验收）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 echo PTY 构造 headless GPUI app，并进入空 Canvas。
///   2. 真实模拟 Cmd+1 到 Cmd+6，经过 root on_key_down、shortcut registry 与 action handler。
///   3. 读取 Runtime document，分别统计 Terminal 和五种 Agent profile。
///   4. 再模拟旧 Cmd+N，确认节点总数不增加。
///
/// 预期结果：
///   - 正断言：恰好创建 1 个 Terminal，以及 Claude/Codex/Cursor/OpenCode/Grok 各 1 个。
///   - 负断言：不存在 Preview/Browser/Rect 等错误节点，旧 Cmd+N 不再创建第七个节点。
#[test]
fn e2e_card_shortcuts_cmd_1_through_cmd_6_create_profiles_in_product_order() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Cards 快捷键 e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    cx.simulate_keystrokes("cmd-1 cmd-2 cmd-3 cmd-4 cmd-5 cmd-6");
    cx.run_until_parked();

    let nodes = read_runtime(&app, cx, |runtime| runtime.document.nodes().to_vec());
    let terminal_count = nodes
        .iter()
        .filter(|node| matches!(node.kind, CanvasNodeKind::Term(_)))
        .count();
    let claude_count = nodes
        .iter()
        .filter(|node| {
            matches!(
                &node.kind,
                CanvasNodeKind::Agent(spec) if spec.profile == AgentProfile::Claude
            )
        })
        .count();
    let codex_count = nodes
        .iter()
        .filter(|node| {
            matches!(
                &node.kind,
                CanvasNodeKind::Agent(spec) if spec.profile == AgentProfile::Codex
            )
        })
        .count();
    let cursor_count = nodes
        .iter()
        .filter(|node| {
            matches!(
                &node.kind,
                CanvasNodeKind::Agent(spec) if spec.profile == AgentProfile::Cursor
            )
        })
        .count();
    let opencode_count = nodes
        .iter()
        .filter(|node| {
            matches!(
                &node.kind,
                CanvasNodeKind::Agent(spec) if spec.profile == AgentProfile::OpenCode
            )
        })
        .count();
    let grok_count = nodes
        .iter()
        .filter(|node| {
            matches!(
                &node.kind,
                CanvasNodeKind::Agent(spec) if spec.profile == AgentProfile::Grok
            )
        })
        .count();
    let unexpected_count = nodes
        .iter()
        .filter(|node| {
            matches!(
                node.kind,
                CanvasNodeKind::Rect(_) | CanvasNodeKind::Preview(_) | CanvasNodeKind::Browser(_)
            )
        })
        .count();

    assert_eq!(
        nodes.len(),
        6,
        "六个数字快捷键应创建恰好 6 个节点，实际为 {}",
        nodes.len()
    );
    assert_eq!(
        terminal_count, 1,
        "Cmd+1 应创建恰好 1 个 Terminal，实际为 {terminal_count}"
    );
    assert_eq!(
        claude_count, 1,
        "Cmd+2 应创建恰好 1 个 Claude Agent，实际为 {claude_count}"
    );
    assert_eq!(
        codex_count, 1,
        "Cmd+3 应创建恰好 1 个 Codex Agent，实际为 {codex_count}"
    );
    assert_eq!(
        cursor_count, 1,
        "Cmd+4 应创建恰好 1 个 Cursor Agent，实际为 {cursor_count}"
    );
    assert_eq!(
        opencode_count, 1,
        "Cmd+5 应创建恰好 1 个 OpenCode Agent，实际为 {opencode_count}"
    );
    assert_eq!(
        grok_count, 1,
        "Cmd+6 应创建恰好 1 个 Grok Agent，实际为 {grok_count}"
    );
    assert_eq!(
        unexpected_count, 0,
        "数字快捷键不应误建 Preview、Browser 或 Rect，实际误建 {unexpected_count} 个"
    );

    cx.simulate_keystrokes("cmd-n");
    cx.run_until_parked();
    let count_after_old_shortcut = read_runtime(&app, cx, |runtime| runtime.document.nodes().len());
    assert_eq!(
        count_after_old_shortcut, 6,
        "旧 Cmd+N 不应继续创建 Terminal；节点数应保持 6，实际为 {count_after_old_shortcut}"
    );
}
