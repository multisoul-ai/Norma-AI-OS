//! Agent 编排结果闭环：可信 Stop 回复必须由 wait/status 与 result 命令按 task_ref 返回。

use gpui::TestAppContext;
use norma::protocol::{
    AgentProfile, AgentTaskAssignment, AgentTaskReceiptStatus, AgentTaskResultAvailability,
    CardCommand, CardCommandResult,
};
use std::time::Duration;

use crate::harness::{drain_until, echo_factory, enter_canvas, make_app};

/// E2E-RESULT-01：同一任务的终态状态与独立 result 查询返回完全相同的 Agent 回复。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent = Bella/Codex，node 数 = 1
///   dispatched tasks = 1，task_ref 取派发回执中的不透明 ID
///   Stop.final_response = "检查完成：3 项通过，0 项失败。"
///   回复字符数 < 1200 上限 → availability=available、truncated=false
///   提交等待预算 = 120ms > 输入后 Enter 延迟 100ms → 至少跨过 1 次提交截止点
///   frame 等待预算 = 500ms：约 500 / 16 ≈ 31 个 Runtime frame
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建并激活 Bella，注入 SessionStart 使 Agent 会话进入可提交状态。
///   2. 通过公开 DispatchAgentTasks 派发一项 managed 任务并保存 task_ref。
///   3. 在任务未完成时查询 result，确认明确返回未就绪错误。
///   4. 注入 UserPromptSubmit，确认当前任务已经由 Provider 接收并进入 Thinking。
///   5. 注入携带 final_response 的可信 Stop，等待 Runtime 推进到 Done。
///   6. 分别执行 AgentTaskStatus（wait 的轮询命令）与 AgentTaskResult。
///
/// 预期结果：
///   - 正断言：status 为 Done，且直接携带 available、未截断的精确回复。
///   - 正断言：result 命令返回与 status 完全相同的 task/result 对象。
///   - 负断言：非终态不得伪造结果；终态结果不得为空、串号或退化成 unavailable。
#[test]
fn terminal_status_and_result_command_return_the_same_bound_reply() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("任务结果 e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    let created = execute(
        &app,
        cx,
        CardCommand::CreateAgent {
            name: Some("Bella".to_string()),
            profile: AgentProfile::Codex,
            activate: true,
        },
    )
    .expect("应创建并激活 Bella");
    let CardCommandResult::AgentCreated {
        node_id: Some(node_id),
        ..
    } = created
    else {
        panic!("激活创建 Bella 必须返回 node_id");
    };

    let session_start = norma::agent_hook::parse_hook_event(
        norma::agent_hook::HookSource::Codex,
        node_id,
        serde_json::json!({
            "hook_event_name": "SessionStart",
            "session_id": "codex-result-session"
        }),
    )
    .expect("SessionStart 应可解析");
    app.update(cx, |app, _cx| {
        app.enqueue_agent_hook_event_for_e2e(session_start)
            .expect("SessionStart 应进入 Hook 队列");
    });
    let _ = drain_until(
        &app,
        cx,
        |runtime| {
            (runtime.agent_session_id(node_id).as_deref() == Some("codex-result-session"))
                .then_some(())
        },
        Duration::from_millis(500),
    )
    .expect("约 31 个 Runtime frame 内应缓存 SessionStart");

    let dispatched = execute(
        &app,
        cx,
        CardCommand::DispatchAgentTasks {
            assignments: vec![AgentTaskAssignment {
                name: "Bella".to_string(),
                task: "检查当前项目".to_string(),
            }],
            force_untracked: false,
        },
    )
    .expect("Bella 应接收 managed 任务");
    let CardCommandResult::AgentTasksDispatched { receipts } = dispatched else {
        panic!("派发必须返回 AgentTasksDispatched");
    };
    assert_eq!(receipts.len(), 1, "单个 assignment 必须只产生一条回执");
    let task_ref = receipts[0].task_id.clone();
    let not_ready = execute(
        &app,
        cx,
        CardCommand::AgentTaskResult {
            task_ref: task_ref.clone(),
        },
    )
    .expect_err("非终态 result 查询必须失败而不是伪造回复");
    assert!(
        not_ready.contains("任务尚未进入终态"),
        "非终态 result 必须明确说明结果尚未就绪，实际：{not_ready}"
    );
    assert!(
        !not_ready.contains("未找到"),
        "任务已存在但仍在执行时不得误报 task not found，实际：{not_ready}"
    );

    std::thread::sleep(Duration::from_millis(120));
    cx.cx
        .background_executor
        .advance_clock(Duration::from_millis(16));
    cx.run_until_parked();

    let prompt_submit = norma::agent_hook::parse_hook_event(
        norma::agent_hook::HookSource::Codex,
        node_id,
        serde_json::json!({
            "hook_event_name": "UserPromptSubmit",
            "session_id": "codex-result-session",
            "prompt": "检查当前项目"
        }),
    )
    .expect("UserPromptSubmit 应可解析");
    app.update(cx, |app, _cx| {
        app.enqueue_agent_hook_event_for_e2e(prompt_submit)
            .expect("UserPromptSubmit 应进入 Hook 队列");
    });
    let _ = drain_until(
        &app,
        cx,
        |runtime| {
            (runtime.agent_task_state(node_id) == Some(runtime::AgentTaskState::Thinking))
                .then_some(())
        },
        Duration::from_millis(500),
    )
    .expect("约 31 个 Runtime frame 内 UserPromptSubmit 应推进到 Thinking");

    let stop = norma::agent_hook::parse_hook_event(
        norma::agent_hook::HookSource::Codex,
        node_id,
        serde_json::json!({
            "hook_event_name": "Stop",
            "session_id": "codex-result-session",
            "final_response": "检查完成：3 项通过，0 项失败。"
        }),
    )
    .expect("携带 final_response 的 Stop 应可解析");
    app.update(cx, |app, _cx| {
        app.enqueue_agent_hook_event_for_e2e(stop)
            .expect("Stop 应进入 Hook 队列");
    });
    let _ = drain_until(
        &app,
        cx,
        |runtime| {
            (runtime.agent_task_state(node_id) == Some(runtime::AgentTaskState::Done)).then_some(())
        },
        Duration::from_millis(500),
    )
    .expect("约 31 个 Runtime frame 内 Stop 应推进到 Done");

    let status = execute(
        &app,
        cx,
        CardCommand::AgentTaskStatus {
            task_ref: task_ref.clone(),
        },
    )
    .expect("终态 status 应可查询");
    let CardCommandResult::AgentTaskStatus { task: status_task } = status else {
        panic!("status 必须返回 AgentTaskStatus");
    };
    assert_eq!(
        status_task.status,
        AgentTaskReceiptStatus::Done,
        "可信 Stop 后 wait/status 必须返回 Done"
    );
    let result = execute(
        &app,
        cx,
        CardCommand::AgentTaskResult {
            task_ref: task_ref.clone(),
        },
    )
    .expect("终态 result 应可查询");
    let CardCommandResult::AgentTaskResult { task: result_task } = result else {
        panic!("result 必须返回独立 AgentTaskResult 响应");
    };

    let bound = status_task
        .result
        .as_ref()
        .expect("Done 状态必须直接携带绑定结果");
    assert_eq!(
        bound.availability,
        AgentTaskResultAvailability::Available,
        "Stop 提供 final_response 时结果必须 available"
    );
    assert_eq!(
        bound.text.as_deref(),
        Some("检查完成：3 项通过，0 项失败。"),
        "wait/status 必须返回 Stop 中的精确最终回复"
    );
    assert!(!bound.truncated, "短回复不得错误标记为截断");
    assert_eq!(
        result_task, status_task,
        "result 命令必须与 wait/status 读取同一权威任务结果"
    );
    assert_eq!(
        result_task.task_id, task_ref,
        "结果必须绑定原派发回执的 task_ref，不得串到其他任务"
    );
    assert_ne!(
        result_task
            .result
            .expect("result 查询必须返回结果")
            .availability,
        AgentTaskResultAvailability::Unavailable,
        "有 final_response 时 result 不得退化为 unavailable"
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
