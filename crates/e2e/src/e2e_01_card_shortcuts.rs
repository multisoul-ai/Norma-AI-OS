//! Cards 数字快捷键的 L1 验收：真实键事件必须创建对应的六类可运行卡片。

use canvas::{AgentProfile, CanvasNodeKind};
use gpui::TestAppContext;

use crate::harness::{echo_factory, enter_canvas, make_app, read_runtime};

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
