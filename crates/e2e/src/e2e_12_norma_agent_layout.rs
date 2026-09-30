use crate::{echo_factory, enter_canvas, make_app, read_runtime};
use agent::AdapterCommand;
use canvas::CanvasNodeKind;
use gpui::TestAppContext;
use norma::protocol::AgentProfile;

/// E2E-NORMA-LAYOUT-1：Norma 连续创建 AgentCard 时，前三张必须进入默认的每行三张网格。
///
/// 数据构造（含关键数值的推导过程）：
///   初始画布节点数 = 0
///   Norma `create_agent` 调用 = 3（Codex + Claude + Cursor）
///   默认布局 = RowFirst、每行 3 张
///   index=0 → 第 0 行第 0 列；index=1 → 第 0 行第 1 列；index=2 → 第 0 行第 2 列
///   预期 AgentCard 数 = 0 + 3 = 3
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless 真实 MultisoulApp 并进入临时项目 Canvas。
///   2. 经 production `AgentCardAdapter` 依次执行三次 `create_agent`。
///   3. 从 Runtime 画布节点投影读取三张创建回执对应的最终 bounds。
///   4. 比较尺寸与坐标，验证前三张从左向右填满第一行。
///
/// 预期结果：
///   - 正断言：三次创建都成功且画布精确存在三张 AgentCard。
///   - 正断言：三张卡片尺寸相同；第二张在第一张右侧，第三张在第二张右侧。
///   - 负断言：三张卡片的 origin 必须逐一不同，避免新建卡片覆盖已有卡片。
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
                    task: None,
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
                    task: None,
                },
            )
        })
    });
    let third = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            app.execute_norma_adapter_command_for_e2e(
                window,
                cx,
                AdapterCommand::CreateAgent {
                    profile: AgentProfile::Cursor,
                    task: None,
                },
            )
        })
    });

    let first = first.expect("first Norma create_agent should succeed");
    let second = second.expect("second Norma create_agent should succeed");
    let third = third.expect("third Norma create_agent should succeed");
    let first_node_id = first
        .node_id
        .expect("first Norma Agent creation should return its node id");
    let second_node_id = second
        .node_id
        .expect("second Norma Agent creation should return its node id");
    let third_node_id = third
        .node_id
        .expect("third Norma Agent creation should return its node id");
    let (first_bounds, second_bounds, third_bounds, agent_cards) =
        read_runtime(&app, cx, |runtime| {
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
            let third_bounds = runtime
                .document
                .node(third_node_id)
                .expect("third Norma-created AgentCard should remain on the Canvas")
                .bounds;
            let agent_cards = runtime
                .document
                .nodes()
                .iter()
                .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
                .count();
            (first_bounds, second_bounds, third_bounds, agent_cards)
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
        third.status, "succeeded",
        "third Norma create_agent must report a successful App mutation"
    );
    assert_eq!(
        agent_cards, 3,
        "three Norma create_agent calls must create exactly three AgentCards"
    );
    assert_eq!(
        first_bounds.size, second_bounds.size,
        "the first two AgentCards in the default grid must use the same card size"
    );
    assert_eq!(
        first_bounds.size, third_bounds.size,
        "the third AgentCard must use the same fitted size as the first two"
    );
    assert_eq!(
        second_bounds.origin.y, first_bounds.origin.y,
        "RowFirst must keep the first two AgentCards in the same row"
    );
    assert!(
        second_bounds.origin.x > first_bounds.origin.x,
        "the second AgentCard must occupy the grid cell to the right of the first"
    );
    assert!(
        third_bounds.origin.x > second_bounds.origin.x,
        "the third AgentCard must occupy the third column to the right of the second"
    );
    assert_eq!(
        third_bounds.origin.y, first_bounds.origin.y,
        "the third AgentCard must remain in the first row"
    );
    assert_ne!(
        first_bounds.origin, second_bounds.origin,
        "the second AgentCard must not reuse the first card's full-canvas origin"
    );
    assert_ne!(
        first_bounds.origin, third_bounds.origin,
        "the third AgentCard must not cover the first AgentCard"
    );
    assert_ne!(
        second_bounds.origin, third_bounds.origin,
        "the third AgentCard must not cover the second AgentCard"
    );
}
