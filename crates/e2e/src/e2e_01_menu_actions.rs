//! 原生菜单 Action 的 L1 验收：OpenCode 创建命令必须到达 Canvas root。

use app::NewOpenCodeAgent;
use canvas::{AgentProfile, CanvasNodeKind};
use gpui::TestAppContext;

use crate::harness::{echo_factory, enter_canvas, make_app, read_runtime};

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
