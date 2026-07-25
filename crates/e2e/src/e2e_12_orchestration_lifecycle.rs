//! Agent 编排生命周期：自动激活、可查询 pending 状态和精确取消。

use gpui::TestAppContext;
use norma::protocol::{
    AgentProfile, AgentTaskAssignment, AgentTaskReceiptStatus, CardCommand, CardCommandResult,
};

use crate::harness::{echo_factory, enter_canvas, make_app};

/// 尚未提交的自动激活任务：status 必须为 queued，精确取消必须直接进入 cancelled。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent 定义 = 1：Bella/Codex，active_node_id=None。
///   assignment = 1：Bella ← “验证安装流程”。
///   startup grace = 30s；测试不发送 SessionStart，因此任务尚未输入或提交。
///   task_ref = `<runtime_instance_id>:1`，后续 status/cancel 复用同一不透明引用。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建已启用但未激活的 Bella 定义。
///   2. 派发单个任务，复用生产协调器并自动创建 Bella 卡片。
///   3. 在 CLI Ready 前按 task_ref 查询状态。
///   4. 按同一 task_ref 精确取消 pending 任务，再次查询终态。
///
/// 预期结果：
///   - 正断言：Bella 自动激活，派发回执和查询状态都为 queued。
///   - 正断言：取消返回 cancelled，之后状态仍可精确查询为 cancelled。
///   - 负断言：pending 任务不得进入 cancel_requested，也不得影响其他任务。
#[test]
fn e2e_12_pending_orchestration_task_cancels_without_false_interrupt() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("编排生命周期 e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    let created = cx.update(|window, cx| {
        app.update(cx, |app, _cx| {
            app.execute_agent_card_command_for_e2e(
                window,
                CardCommand::CreateAgent {
                    name: Some("Bella".to_string()),
                    profile: AgentProfile::Codex,
                    activate: false,
                },
            )
        })
    });
    assert!(
        matches!(
            created,
            Ok(CardCommandResult::AgentCreated { node_id: None, .. })
        ),
        "Bella 应先作为已启用但未激活的定义创建，actual={created:?}"
    );

    let dispatched = cx.update(|window, cx| {
        app.update(cx, |app, _cx| {
            app.execute_agent_card_command_for_e2e(
                window,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "Bella".to_string(),
                        task: "验证安装流程".to_string(),
                    }],
                    force_untracked: false,
                },
            )
        })
    });
    let CardCommandResult::AgentTasksDispatched { receipts } =
        dispatched.expect("Bella 自动激活并接收任务应成功")
    else {
        panic!("派发必须返回 AgentTasksDispatched");
    };
    assert_eq!(receipts.len(), 1, "一次 assign 只能产生一条任务回执");
    let receipt = receipts[0].clone();
    assert!(receipt.node_id > 0, "未激活 Bella 必须自动创建唯一有效卡片");
    assert_eq!(
        receipt.status,
        AgentTaskReceiptStatus::Queued,
        "CLI Ready 前任务应真实报告 queued，不能谎报 submitted"
    );

    let before_cancel = execute(
        &app,
        cx,
        CardCommand::AgentTaskStatus {
            task_ref: receipt.task_id.clone(),
        },
    )
    .expect("当前运行实例的 pending task_ref 应可查询");
    let CardCommandResult::AgentTaskStatus { task } = before_cancel else {
        panic!("status 必须返回 AgentTaskStatus");
    };
    assert_eq!(
        task.status,
        AgentTaskReceiptStatus::Queued,
        "尚未真实提交的任务查询状态必须保持 queued"
    );
    assert_ne!(
        task.status,
        AgentTaskReceiptStatus::Submitted,
        "pending 状态不得被查询路径误报为 submitted"
    );

    let cancelled = execute(
        &app,
        cx,
        CardCommand::AgentTaskCancel {
            task_ref: receipt.task_id.clone(),
        },
    )
    .expect("pending 任务应可精确取消");
    assert_eq!(
        cancelled,
        CardCommandResult::AgentTaskCancelled {
            task_id: receipt.task_id.clone(),
            status: AgentTaskReceiptStatus::Cancelled,
        },
        "尚未提交的任务必须直接进入 cancelled"
    );

    let after_cancel = execute(
        &app,
        cx,
        CardCommand::AgentTaskStatus {
            task_ref: receipt.task_id,
        },
    )
    .expect("取消后的任务终态应仍可按引用查询");
    let CardCommandResult::AgentTaskStatus { task } = after_cancel else {
        panic!("取消后 status 必须返回 AgentTaskStatus");
    };
    assert_eq!(
        task.status,
        AgentTaskReceiptStatus::Cancelled,
        "精确取消后任务必须保持 cancelled 终态"
    );
    assert_ne!(
        task.status,
        AgentTaskReceiptStatus::CancelRequested,
        "尚未提交任务不得进入运行中取消的 cancel_requested"
    );
}

fn execute(
    app: &gpui::Entity<app::MultisoulApp>,
    cx: &mut gpui::VisualTestContext,
    command: CardCommand,
) -> Result<CardCommandResult, String> {
    cx.update(|window, cx| {
        app.update(cx, |app, _cx| {
            app.execute_agent_card_command_for_e2e(window, command)
        })
    })
}
