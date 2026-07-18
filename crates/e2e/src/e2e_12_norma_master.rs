use crate::{echo_factory, enter_canvas, make_app, read_runtime};
use agent::AdapterCommand;
use canvas::CanvasNodeKind;
use gpui::TestAppContext;
use norma::protocol::AgentProfile;

/// E2E-NORMA-1：Norma 的生产 Adapter 用内置名称创建 AgentCard，并直接派发单 Agent 任务
///
/// 数据构造（含关键数值的推导过程）：
///   Fresh project definitions = 0
///   Create calls = 1（Codex）
///   Assign calls = 1（创建所得内置名称）
///   Expected Canvas AgentCards = 1 + 0 reused by assignment = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless 真实 MultisoulApp 并进入临时项目 Canvas
///   2. 经 production `AgentCardAdapter` 执行 `create_agent(codex)`
///   3. 从 Agent 设置投影读取系统生成的名称，再立即执行 `assign_agent_task`
///
/// 预期结果：
///   - 正断言：创建成功、任务 accepted 且返回 task_id=1
///   - 正断言：画布只有一张 AgentCard，派发复用该卡片
///   - 负断言：系统生成的 Agent 不能名为 Norma
#[test]
fn norma_master_create_then_direct_assign_reaches_real_app_fifo() {
    let mut gpui = TestAppContext::single();
    let (app, cx) = make_app(&mut gpui, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Norma e2e workspace should be creatable");
    enter_canvas(&app, cx, workspace.path());

    let created = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            app.execute_norma_adapter_command_for_e2e(
                window,
                AdapterCommand::CreateAgent {
                    profile: AgentProfile::Codex,
                },
            )
        })
    });
    let created = created.expect("production Norma create_agent should succeed");
    let definition = app.read_with(cx, |app, _cx| {
        app.agent_settings_e2e_snapshot().definitions[0].clone()
    });
    let assigned = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            app.execute_norma_adapter_command_for_e2e(
                window,
                AdapterCommand::AssignAgentTask {
                    agent: definition.name.clone(),
                    task: "检查当前项目有哪些技术栈。".to_string(),
                },
            )
        })
    });
    let assigned = assigned.expect("production Norma assign_agent_task should succeed directly");
    let agent_cards = read_runtime(&app, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
            .count()
    });

    assert_eq!(
        created.status, "succeeded",
        "create_agent should report a real successful App mutation"
    );
    assert_eq!(
        assigned.status, "accepted",
        "assign_agent_task should report acceptance without waiting for task completion"
    );
    assert_eq!(
        assigned.task_id,
        Some(1),
        "the first real AgentCard FIFO task should expose task_id=1"
    );
    assert_eq!(
        agent_cards, 1,
        "direct assignment should reuse the single AgentCard opened during creation"
    );
    assert!(
        !definition.name.eq_ignore_ascii_case("Norma"),
        "the Master identity must not be allocated as a Canvas Agent name"
    );
}
