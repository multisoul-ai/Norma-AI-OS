use card_task::{
    CardCommandRunner, CardTaskEvent, CardTaskPlanner, CardTaskPlannerError, CardTaskQueue,
    CardTaskRequest, CardTaskState,
};
use norma::protocol::{CardCommand, CardCommandError, CardCommandResult, CardKind, Placement};
use std::collections::VecDeque;
use voice::{CompiledVoicePlan, ExecutableCommand, RiskLevel};

#[derive(Default)]
struct FakePlanner {
    results: VecDeque<Result<CompiledVoicePlan, CardTaskPlannerError>>,
}

impl FakePlanner {
    fn with_results(results: Vec<Result<CompiledVoicePlan, CardTaskPlannerError>>) -> Self {
        Self {
            results: results.into(),
        }
    }
}

impl CardTaskPlanner for FakePlanner {
    fn plan(
        &mut self,
        _request: &CardTaskRequest,
    ) -> Result<CompiledVoicePlan, CardTaskPlannerError> {
        self.results
            .pop_front()
            .expect("test planner should always have a prepared result")
    }
}

#[derive(Default)]
struct FakeRunner {
    commands: Vec<CardCommand>,
    results: VecDeque<Result<CardCommandResult, CardCommandError>>,
}

impl FakeRunner {
    fn with_results(results: Vec<Result<CardCommandResult, CardCommandError>>) -> Self {
        Self {
            commands: Vec::new(),
            results: results.into(),
        }
    }
}

impl CardCommandRunner for FakeRunner {
    fn run(&mut self, command: CardCommand) -> Result<CardCommandResult, CardCommandError> {
        self.commands.push(command.clone());
        if let Some(result) = self.results.pop_front() {
            return result;
        }

        match command {
            CardCommand::Create { .. } => Ok(CardCommandResult::Created { node_id: 512 }),
            CardCommand::SendText { node_id, .. } => Ok(CardCommandResult::Sent { node_id }),
            CardCommand::Focus { node_id } => Ok(CardCommandResult::Focused { node_id }),
            CardCommand::Delete { node_id } => Ok(CardCommandResult::Deleted { node_id }),
            CardCommand::List => Ok(CardCommandResult::Listed { cards: Vec::new() }),
        }
    }
}

fn request(intent: &str) -> CardTaskRequest {
    CardTaskRequest {
        intent: intent.to_string(),
        tool_call_id: None,
    }
}

fn create_then_send_then_focus_plan() -> CompiledVoicePlan {
    CompiledVoicePlan {
        commands: vec![
            ExecutableCommand::Card(CardCommand::Create {
                kind: CardKind::Terminal { shell: None },
                placement: Placement::VisibleArea,
            }),
            ExecutableCommand::SendTextToPrevious {
                previous_step: 0,
                text: "请分析 README".to_string(),
                submit: true,
            },
            ExecutableCommand::FocusPrevious { previous_step: 0 },
        ],
        requires_confirmation: false,
        risk: RiskLevel::Low,
        reason: Some("create send focus".to_string()),
    }
}

/// QS-1: SendText step 失败后，task 必须终止，后续 FocusPrevious 不能继续执行。
///
/// 数据构造（含关键数值的推导过程）：
///   task steps = 3
///   step0 = Create，fake runner 返回 Created(node_id=512)
///   step1 = SendTextToPrevious(step0, submit=true)，fake runner 返回 Runtime("send exploded")
///   step2 = FocusPrevious(step0)，按验收要求这一步不应被执行
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 第一次 run_next()：task 规划并执行 step0(create)
///   2. 第二次 run_next()：task 执行 step1(send)，收到 runner 错误
///   3. 检查失败事件、终态，以及 runner 实际收到的命令序列
///
/// 预期结果：
///   - 正断言：第二次 run_next 包含 StepFailed(step1, "send exploded")
///   - 正断言：第二次 run_next 包含 Failed("send exploded")
///   - 正断言：runner 第 2 条命令是 SendText(node_id=512, submit=true)
///   - 负断言：runner 不应继续执行 step2 的 Focus
#[test]
fn test_send_failure_stops_later_steps() {
    let mut queue = CardTaskQueue::default();
    let task_id = queue.enqueue(request("创建后发送 README 分析"));
    let mut planner = FakePlanner::with_results(vec![Ok(create_then_send_then_focus_plan())]);
    let mut runner = FakeRunner::with_results(vec![
        Ok(CardCommandResult::Created { node_id: 512 }),
        Err(CardCommandError::Runtime {
            message: "send exploded".to_string(),
        }),
    ]);

    let first_events = queue.run_next(&mut planner, &mut runner);
    let second_events = queue.run_next(&mut planner, &mut runner);

    assert!(
        first_events.iter().any(|event| matches!(event, CardTaskEvent::StepSucceeded { task_id: succeeded_id, step_index } if *succeeded_id == task_id && *step_index == 0)),
        "第一次 run_next 必须先完成 step0(create)，否则后续 send 无法解析 previous node_id"
    );
    assert!(
        second_events.iter().any(|event| matches!(event, CardTaskEvent::StepFailed { task_id: failed_id, step_index, message } if *failed_id == task_id && *step_index == 1 && message.contains("send exploded"))),
        "send 失败时必须先发出 StepFailed(step1, send exploded)"
    );
    assert!(
        second_events.iter().any(|event| matches!(event, CardTaskEvent::Failed { task_id: failed_id, message } if *failed_id == task_id && message.contains("send exploded"))),
        "send 失败后必须继续发出终态 Failed(send exploded)"
    );
    assert_eq!(
        runner.commands[1],
        CardCommand::SendText {
            node_id: 512,
            text: "请分析 README".to_string(),
            submit: true,
        },
        "submit=true 的 SendText 必须带着 create 产出的 node_id=512 透传给 runner"
    );
    assert_eq!(
        runner.commands.len(),
        2,
        "send 失败后任务必须立即停止，不能继续执行 step2 的 FocusPrevious"
    );
    assert_eq!(
        queue.completed_tasks()[0].state,
        CardTaskState::Failed {
            message: "send exploded".to_string(),
        },
        "send 失败后的 completed task 终态必须是 Failed(send exploded)"
    );
}

/// QS-2: active task 在 create 后、send 前取消时，不回滚已创建卡片，也不继续 send。
///
/// 数据构造（含关键数值的推导过程）：
///   task steps = 3
///   step0 = Create，fake runner 返回 Created(node_id=512)
///   step1 = SendTextToPrevious(step0, submit=true)
///   step2 = FocusPrevious(step0)
///   cancel 时 completed_steps = 1，因此取消摘要必须说明“已完成工作不会回滚”
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 第一次 run_next()：执行 step0(create)，task 保持 active
///   2. cancel_latest()：对 active task 发起取消请求
///   3. 第二次 run_next()：在 step 边界真正停止 task
///   4. 检查 runner 只收到 Create，且取消消息带“不回滚”提示
///
/// 预期结果：
///   - 正断言：cancel_latest 返回的 Cancelled 消息包含 "not rolled back"
///   - 正断言：第二次 run_next 返回的 Cancelled 消息包含 "not rolled back"
///   - 正断言：runner 只收到 Create，不收到 SendText 或 Delete
///   - 正断言：completed task 终态为 Cancelled，且消息包含 "not rolled back"
#[test]
fn test_cancel_after_create_before_send_keeps_created_card_and_skips_send() {
    let mut queue = CardTaskQueue::default();
    let task_id = queue.enqueue(request("创建后发送 README 分析"));
    let mut planner = FakePlanner::with_results(vec![Ok(create_then_send_then_focus_plan())]);
    let mut runner =
        FakeRunner::with_results(vec![Ok(CardCommandResult::Created { node_id: 512 })]);

    let first_events = queue.run_next(&mut planner, &mut runner);
    let cancel_event = queue
        .cancel_latest()
        .expect("存在 active task 时 cancel_latest 应返回取消事件");
    let second_events = queue.run_next(&mut planner, &mut runner);

    assert!(
        first_events.iter().any(|event| matches!(event, CardTaskEvent::StepSucceeded { task_id: succeeded_id, step_index } if *succeeded_id == task_id && *step_index == 0)),
        "取消前必须先完成 step0(create)，这个场景才是在 create 后、send 前取消"
    );
    assert!(
        matches!(&cancel_event, CardTaskEvent::Cancelled { task_id: cancelled_id, message } if *cancelled_id == task_id && message.contains("not rolled back")),
        "active task 已完成至少一步时，cancel_latest 的摘要必须明确说明 completed work was not rolled back"
    );
    assert!(
        second_events.iter().any(|event| matches!(event, CardTaskEvent::Cancelled { task_id: cancelled_id, message } if *cancelled_id == task_id && message.contains("not rolled back"))),
        "真正落地的 Cancelled 终态消息也必须说明 completed work was not rolled back"
    );
    assert_eq!(
        runner.commands.len(),
        1,
        "create 后取消不应继续 send，也不应额外执行任何回滚命令"
    );
    assert!(
        matches!(&runner.commands[0], CardCommand::Create { .. }),
        "取消后保留下来的唯一副作用必须是已完成的 Create"
    );
    assert!(
        runner
            .commands
            .iter()
            .all(|command| !matches!(command, CardCommand::SendText { .. })),
        "create 后、send 前取消时，runner 绝不能收到 SendText"
    );
    assert!(
        runner
            .commands
            .iter()
            .all(|command| !matches!(command, CardCommand::Delete { .. })),
        "MVP 取消语义明确不回滚，runner 绝不能收到 Delete 回滚命令"
    );
    assert!(
        matches!(&queue.completed_tasks()[0].state, CardTaskState::Cancelled { message } if message.contains("not rolled back")),
        "completed task 的 Cancelled 终态必须保留“不回滚已完成工作”的摘要"
    );
}
