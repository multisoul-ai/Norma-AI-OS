use crate::{echo_factory, enter_canvas, make_app, read_runtime};
use agent::AdapterCommand;
use canvas::CanvasNodeKind;
use gpui::TestAppContext;
use norma::protocol::AgentProfile;

/// E2E-NORMA-LAYOUT-1：Norma 连续创建 AgentCard 时，第二张必须进入与第一张一致的当前网格。
///
/// 数据构造（含关键数值的推导过程）：
///   初始画布节点数 = 0
///   Norma `create_agent` 调用 = 2（Codex + Claude）
///   默认布局 = RowFirst、每行 3 张，因此 index=0 → 第 0 行第 0 列，index=1 → 第 0 行第 1 列
///   预期 AgentCard 数 = 0 + 2 = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless 真实 MultisoulApp 并进入临时项目 Canvas。
///   2. 经 production `AgentCardAdapter` 依次执行 `create_agent(codex)` 与 `create_agent(claude)`。
///   3. 从 Runtime 画布节点投影读取两张创建回执对应的 bounds。
///   4. 比较尺寸与坐标，验证两张卡片遵循同一行优先网格，而非第二张覆盖整个可视画布。
///
/// 预期结果：
///   - 正断言：两次创建都成功且画布精确存在两张 AgentCard。
///   - 正断言：两张 AgentCard 尺寸相同，第二张位于第一张右侧且同一行。
///   - 负断言：两张卡片的 origin 不得相同，避免 Norma 新建卡片仍以全画布 bounds 覆盖已有卡片。
#[test]
fn norma_created_agents_join_the_current_grid_instead_of_maximizing() {
    let mut gpui = TestAppContext::single();
    let (app, cx) = make_app(&mut gpui, echo_factory());
    let workspace =
        tempfile::TempDir::new().expect("Norma grid layout workspace should be creatable");
    enter_canvas(&app, cx, workspace.path());

    let first = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            app.execute_norma_adapter_command_for_e2e(
                window,
                cx,
                AdapterCommand::CreateAgent {
                    profile: AgentProfile::Codex,
                },
            )
        })
    });
    let second = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            app.execute_norma_adapter_command_for_e2e(
                window,
                cx,
                AdapterCommand::CreateAgent {
                    profile: AgentProfile::Claude,
                },
            )
        })
    });

    let first = first.expect("first Norma create_agent should succeed");
    let second = second.expect("second Norma create_agent should succeed");
    let first_node_id = first
        .node_id
        .expect("first Norma Agent creation should return its node id");
    let second_node_id = second
        .node_id
        .expect("second Norma Agent creation should return its node id");
    let (first_bounds, second_bounds, agent_cards) = read_runtime(&app, cx, |runtime| {
        let first_bounds = runtime
            .document
            .node(first_node_id)
            .expect("first Norma-created AgentCard should remain on the Canvas")
            .bounds;
        let second_bounds = runtime
            .document
            .node(second_node_id)
            .expect("second Norma-created AgentCard should remain on the Canvas")
            .bounds;
        let agent_cards = runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
            .count();
        (first_bounds, second_bounds, agent_cards)
    });

    assert_eq!(
        first.status, "succeeded",
        "first Norma create_agent must report a successful App mutation"
    );
    assert_eq!(
        second.status, "succeeded",
        "second Norma create_agent must report a successful App mutation"
    );
    assert_eq!(
        agent_cards, 2,
        "two Norma create_agent calls must create exactly two AgentCards"
    );
    assert_eq!(
        first_bounds.size, second_bounds.size,
        "AgentCards in the same default grid row must use the same card size"
    );
    assert!(
        second_bounds.origin.x > first_bounds.origin.x,
        "the second AgentCard must occupy the grid cell to the right of the first"
    );
    assert_eq!(
        second_bounds.origin.y, first_bounds.origin.y,
        "the first two AgentCards must stay in the same RowFirst grid row"
    );
    assert_ne!(
        first_bounds.origin, second_bounds.origin,
        "the second AgentCard must not reuse the first card's full-canvas origin"
    );
}
