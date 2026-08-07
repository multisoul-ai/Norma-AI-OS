//! 原生菜单 Action 的 L1 验收：OpenCode 与 Cursor 创建命令必须到达 Canvas root。

use app::{NewCursorAgent, NewGrokAgent, NewOpenCodeAgent};
use canvas::{AgentProfile, CanvasNodeKind};
use gpui::TestAppContext;

use crate::harness::{echo_factory, enter_canvas, make_app, read_runtime};

/// E2E-MENU-GROK：File › New 共享的 `NewGrokAgent` Action 应创建 Grok Build 卡片。
///
/// 数据构造（含关键数值的推导过程）：
///   初始 Agent 节点数 = 0（fresh Launchpad + 临时 workspace）
///   dispatch 次数 = 1（唯一一次 NewGrokAgent）
///   预期 Agent 节点数 = 0 + 1 = 1
///   预期 profile = AgentProfile::Grok（Grok Build 独立 runtime profile）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用确定性 echo PTY 构造 headless GPUI app 并进入 Canvas；
///   2. dispatch 公开 `NewGrokAgent` Action；
///   3. Action 经 root handler 调用 `spawn_agent_card(Grok)`；
///   4. 读取 Runtime document 中的新节点。
///
/// 预期结果：
///   - 正断言：恰好创建一张 Grok Agent 卡片；
///   - 负断言：不得误建 Codex profile，也不得额外创建 Terminal。
#[test]
fn e2e_menu_new_grok_agent_dispatches_to_canvas_root() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Grok 菜单 Action 测试应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    cx.dispatch_action(NewGrokAgent);
    cx.run_until_parked();

    let nodes = read_runtime(&app, cx, |runtime| runtime.document.nodes().to_vec());
    let agent_nodes: Vec<_> = nodes
        .iter()
        .filter_map(|node| match &node.kind {
            CanvasNodeKind::Agent(spec) => Some(spec),
            _ => None,
        })
        .collect();
    let terminal_count = nodes
        .iter()
        .filter(|node| matches!(node.kind, CanvasNodeKind::Term(_)))
        .count();

    assert_eq!(
        agent_nodes.len(),
        1,
        "dispatch NewGrokAgent 后应恰好创建 1 个 Agent 节点，实际为 {}",
        agent_nodes.len()
    );
    assert_eq!(
        agent_nodes[0].profile,
        AgentProfile::Grok,
        "NewGrokAgent 必须创建 Grok profile"
    );
    assert_ne!(
        agent_nodes[0].profile,
        AgentProfile::Codex,
        "NewGrokAgent 不能错误复用 Codex profile"
    );
    assert_eq!(
        terminal_count, 0,
        "NewGrokAgent 不应创建 Terminal 节点，实际 Terminal 数为 {}",
        terminal_count
    );
}

/// E2E-MENU-01：从 File › New 共享的 `NewOpenCodeAgent` Action 创建 OpenCode Agent 节点。
///
/// 数据构造（含关键数值的推导过程）：
///   初始 Agent 节点数 = 0（fresh Launchpad + 无快照测试 workspace）
///   dispatch 次数 = 1（唯一一次 NewOpenCodeAgent）
///   预期 Agent 节点数 = 0 + 1 = 1
///   预期 profile = AgentProfile::OpenCode（Action 与菜单标签的唯一语义映射）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. `make_app(echo_factory)` 构造已聚焦的 headless GPUI root；初始为 Launchpad。
///   2. `enter_canvas` 注入临时 workspace，使 Canvas 专属 Action 变为可分发状态。
///   3. dispatch 公开 `NewOpenCodeAgent`；GPUI 必须在 root 找到注册的处理器。
///   4. 处理器调用 `spawn_agent_card(OpenCode)`，Runtime 写入一个 Agent 节点。
///
/// 预期结果：
///   - 断言 A：恰好新增一个 Agent 节点，证明 Action 未被原生菜单灰置/吞掉。
///   - 断言 B：新增节点 profile 为 OpenCode，不能误建 Claude 或 Codex。
///   - 断言 C：不存在 Terminal 节点，证明该 Action 未错误复用默认 Terminal 创建路径。
#[test]
fn e2e_menu_new_opencode_agent_dispatches_to_canvas_root() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("菜单 Action 测试应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    cx.dispatch_action(NewOpenCodeAgent);
    cx.run_until_parked();

    let nodes = read_runtime(&app, cx, |runtime| runtime.document.nodes().to_vec());
    let agent_nodes: Vec<_> = nodes
        .iter()
        .filter_map(|node| match &node.kind {
            CanvasNodeKind::Agent(spec) => Some(spec),
            _ => None,
        })
        .collect();
    let terminal_count = nodes
        .iter()
        .filter(|node| matches!(node.kind, CanvasNodeKind::Term(_)))
        .count();

    assert_eq!(
        agent_nodes.len(),
        1,
        "dispatch NewOpenCodeAgent 后应恰好创建 1 个 Agent 节点，实际为 {}",
        agent_nodes.len()
    );
    assert_eq!(
        agent_nodes[0].profile,
        AgentProfile::OpenCode,
        "NewOpenCodeAgent 必须创建 OpenCode profile，不能误建其他 Agent"
    );
    assert_ne!(
        agent_nodes[0].profile,
        AgentProfile::Codex,
        "NewOpenCodeAgent 不能错误复用 Codex profile"
    );
    assert_eq!(
        terminal_count, 0,
        "NewOpenCodeAgent 不应创建 Terminal 节点，实际 Terminal 数为 {}",
        terminal_count
    );
}

/// E2E-MENU-02：从 File › New 共享的 `NewCursorAgent` Action 创建 Cursor Agent 节点。
///
/// 数据构造（含关键数值的推导过程）：
///   初始 Agent 节点数 = 0，公开 action dispatch 次数 = 1。
///   预期 Agent 节点数 = 0 + 1 = 1，profile = Cursor。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建带确定性 PTY 的 headless app 并进入 Canvas。
///   2. dispatch `NewCursorAgent`，经真实 Action 路由进入 `spawn_agent_card`。
///   3. 读取 Runtime document 中创建的节点。
///
/// 预期结果：
///   - 正断言：仅创建一个 Agent 节点。
///   - 正断言：节点 profile 为 Cursor。
///   - 负断言：节点不是 Codex，且没有创建 Terminal。
#[test]
fn e2e_menu_new_cursor_agent_dispatches_to_canvas_root() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Cursor 菜单 Action 测试应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    cx.dispatch_action(NewCursorAgent);
    cx.run_until_parked();

    let nodes = read_runtime(&app, cx, |runtime| runtime.document.nodes().to_vec());
    let agent_nodes: Vec<_> = nodes
        .iter()
        .filter_map(|node| match &node.kind {
            CanvasNodeKind::Agent(spec) => Some(spec),
            _ => None,
        })
        .collect();
    let terminal_count = nodes
        .iter()
        .filter(|node| matches!(node.kind, CanvasNodeKind::Term(_)))
        .count();

    assert_eq!(
        agent_nodes.len(),
        1,
        "dispatch NewCursorAgent 后应恰好创建 1 个 Agent 节点，实际为 {}",
        agent_nodes.len()
    );
    assert_eq!(
        agent_nodes[0].profile,
        AgentProfile::Cursor,
        "NewCursorAgent 必须创建 Cursor profile"
    );
    assert_ne!(
        agent_nodes[0].profile,
        AgentProfile::Codex,
        "NewCursorAgent 不能错误复用 Codex profile"
    );
    assert_eq!(
        terminal_count, 0,
        "NewCursorAgent 不应创建 Terminal 节点，实际 Terminal 数为 {}",
        terminal_count
    );
}
