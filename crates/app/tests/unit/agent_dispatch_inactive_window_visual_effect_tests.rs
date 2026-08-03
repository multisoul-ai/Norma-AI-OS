use super::execute_agent_card_command;
use gpui::TestAppContext;
use norma::protocol::{
    AgentProfile, AgentTaskAssignment, AgentTaskReceiptStatus, CardCommand, CardCommandResult,
};

/// DR-T-5：窗口失活时，Canvas 内可见 Agent Card 的成功派发仍必须创建流星回执。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent cards = 1（Alan/Codex，创建在当前可见 Canvas）
///   window_active = false（模拟 Norma OS 位于后台）
///   accepted dispatches = 1（给 Alan 的任务）
///   expected receipts = 1；expected visual effects = 1；expected unavailable skips = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless App，进入 Canvas 并创建唯一 Alan Agent Card。
///   2. 将窗口标记为失活，但保持 Canvas 与目标卡片可见。
///   3. 通过生产命令入口派发一条 Norma 任务，得到最终 Queued receipt。
///   4. 读取 MotionStore presentation 与结构化诊断。
///
/// 预期结果：
///   - 正断言：业务返回唯一 Queued receipt，窗口失活不阻断派发。
///   - 正断言：仍创建唯一 effect，且目标是接收任务的 Alan Agent Card。
///   - 负断言：窗口失活不得计为 unavailable skip，也不得退化为占位目标。
#[test]
fn test_inactive_window_still_spawns_norma_dispatch_visual_effect_for_visible_target() {
    let factory: runtime::PtyFactory = Box::new(|_cmd, _cols, _rows, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)) as Box<dyn terminal::PtyBackend>)
    });
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new()
        .expect("inactive-window dispatch effect test should create a temp workspace");
    let (app, cx) = gpui
        .add_window_view(|_window, cx| crate::app::MultisoulApp::new_with_pty_factory(cx, factory));
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });

    let create_result = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            execute_agent_card_command(
                app,
                window,
                CardCommand::CreateAgent {
                    name: Some("Alan".to_string()),
                    profile: AgentProfile::Codex,
                    activate: true,
                },
            )
        })
    });
    let dispatch_result = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            app.window_active = false;
            execute_agent_card_command(
                app,
                window,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "Alan".to_string(),
                        task: "后台窗口仍需播放流星回执".to_string(),
                    }],
                    force_untracked: false,
                },
            )
        })
    });
    let (presentations, diagnostics) = app.update(cx, |app, _cx| {
        (
            app.motion.dispatch_receipt_presentations(),
            app.motion.diagnostics(),
        )
    });
    let receipt = match &dispatch_result {
        Ok(CardCommandResult::AgentTasksDispatched { receipts }) if receipts.len() == 1 => {
            &receipts[0]
        }
        other => panic!("失活窗口派发必须返回唯一业务 receipt，实际：{other:?}"),
    };

    assert!(
        matches!(create_result, Ok(CardCommandResult::AgentCreated { .. })),
        "失活窗口测试前置必须成功创建 Alan Agent Card；实际：{create_result:?}"
    );
    assert_eq!(
        receipt.status,
        AgentTaskReceiptStatus::Queued,
        "窗口失活不得改变新 Agent 等待 CLI ready 的真实 Queued receipt"
    );
    assert_eq!(
        presentations.len(),
        1,
        "窗口失活但目标可见时，唯一成功 receipt 必须创建唯一流星 effect"
    );
    assert_eq!(
        presentations[0].target_node_id, receipt.node_id,
        "失活窗口的流星 effect 必须命中真实接收任务的 Alan node_id"
    );
    assert_ne!(
        presentations[0].target_node_id, 0,
        "失活窗口的流星 effect 不得退化为占位目标 node_id=0"
    );
    assert_eq!(
        diagnostics.dispatch_unavailable_skips, 0,
        "窗口失活不再属于 unavailable 条件，不得记录视觉跳过"
    );
}
