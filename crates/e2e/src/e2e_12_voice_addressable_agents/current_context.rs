use super::*;
use crate::harness::create_terminal;

/// E2E-A-CONTEXT-01：App 应从当前 workspace 和真实 Canvas 组装轻量上下文。
///
/// 数据构造（含关键数值的推导过程）：
///   workspace = 1 个临时项目
///   cards     = 2 = Terminal(1) + Codex Agent(1)
///   focused   = 1 = 最后创建的 Codex Agent
///   card content fields = 0，上下文只暴露身份、类型、标题和交互状态
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless App 并进入临时 workspace
///   2. 通过公开 action 创建 Terminal 和 Codex Agent
///   3. 读取面向 e2e 的稳定当前画布上下文投影
///
/// 预期结果：
///   - 正断言：项目 root 精确指向当前 workspace
///   - 正断言：Terminal 与 Agent 摘要都存在，Agent 标题为 Alan
///   - 正断言：Agent 聚焦而 Terminal 不聚焦
///   - 负断言：不得凭空出现 Browser 卡片摘要
#[test]
fn app_projects_workspace_and_canvas_cards_into_current_context() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    let terminal_id = create_terminal(&app, cx);
    let agent_id = create_codex_agent(&app, cx);
    let context = app.update(cx, |app, _cx| {
        app.current_canvas_context_e2e()
            .expect("已打开 workspace 应产生当前画布上下文")
    });
    let terminal = context
        .cards
        .iter()
        .find(|card| card.node_id == terminal_id)
        .expect("Terminal node id 应出现在当前画布上下文");
    let agent = context
        .cards
        .iter()
        .find(|card| card.node_id == agent_id)
        .expect("Agent node id 应出现在当前画布上下文");

    assert_eq!(
        context.workspace_root,
        workspace.path().display().to_string(),
        "当前画布上下文的 workspace root 应精确指向已打开项目"
    );
    assert_eq!(
        context.cards.len(),
        2,
        "创建 Terminal 和 Agent 后应恰好产生两张卡片摘要"
    );
    assert_eq!(
        terminal.kind,
        agent::CanvasCardKind::Terminal,
        "Terminal node 应投影为 Terminal 卡片类型"
    );
    assert_eq!(
        terminal.title, "Terminal",
        "Terminal 卡片摘要应使用稳定显示标题"
    );
    assert!(
        !terminal.focused,
        "最后创建 Agent 后 Terminal 不得仍被标记为聚焦"
    );
    assert_eq!(
        agent.kind,
        agent::CanvasCardKind::Agent,
        "Codex Agent node 应投影为 Agent 卡片类型"
    );
    assert_eq!(
        agent.title, "Alan",
        "快速创建的 Codex Agent 应使用正式呼叫名 Alan 作为摘要标题"
    );
    assert!(agent.focused, "最后创建的 Agent 应被标记为聚焦卡片");
    assert!(
        !context
            .cards
            .iter()
            .any(|card| card.kind == agent::CanvasCardKind::Browser),
        "未创建 Browser 时当前画布上下文不得凭空包含 Browser"
    );
}
