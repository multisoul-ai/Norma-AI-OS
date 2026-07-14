use card_task::{
    CardCommandRunner, CardTaskEvent, CardTaskPlanner, CardTaskPlannerError, CardTaskQueue,
    CardTaskRequest,
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
            .expect("fake planner 应预先准备结果")
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
        self.results.pop_front().unwrap_or_else(|| match command {
            CardCommand::Create { .. } => Ok(CardCommandResult::Created { node_id: 700 }),
            CardCommand::Focus { node_id } => Ok(CardCommandResult::Focused { node_id }),
            CardCommand::SendText { node_id, .. } => Ok(CardCommandResult::Sent { node_id }),
            CardCommand::Delete { node_id } => Ok(CardCommandResult::Deleted { node_id }),
            CardCommand::List => Ok(CardCommandResult::Listed { cards: Vec::new() }),
            CardCommand::ListAgents
            | CardCommand::CreateAgent { .. }
            | CardCommand::ActivateAgent { .. }
            | CardCommand::DispatchAgentTasks { .. } => Err(CardCommandError::Unsupported {
                message: "legacy card_task runner does not execute Agent coordination".into(),
            }),
        })
    }
}

fn request(intent: &str) -> CardTaskRequest {
    CardTaskRequest {
        intent: intent.to_string(),
        tool_call_id: Some("call_enqueue".to_string()),
    }
}

fn create_then_focus_plan() -> CompiledVoicePlan {
    CompiledVoicePlan {
        commands: vec![
            ExecutableCommand::Card(CardCommand::Create {
                kind: CardKind::Terminal { shell: None },
                placement: Placement::VisibleArea,
            }),
            ExecutableCommand::FocusPrevious { previous_step: 0 },
        ],
        requires_confirmation: false,
        risk: RiskLevel::Low,
        reason: Some("create then focus".to_string()),
    }
}

/// QC-1：active task 收到取消时，应先发出“已请求取消”，原任务直到 step boundary 才进入终态 Cancelled。
///
/// 数据构造（含关键数值的推导过程）：
///   task steps = 2（Create + FocusPrevious）
///   第一次 run_next 只会完成 step0(create)，因此 next_step_index = 1
///   cancel 时任务仍 active，且已完成步数 = 1，所以终态摘要必须说明已完成工作不会回滚
///
/// 执行过程（逐步说明系统如何处理）：
///   1. enqueue 单个 task，并让 planner 返回两步 compiled plan
///   2. 第一次 run_next() 执行 step0(create)，task 保持 active
///   3. 调用 cancel_latest()，记录立即返回的取消请求事件
///   4. 第二次 run_next()，在 step boundary 真正落地 Cancelled
///
/// 预期结果：
///   - 正断言：cancel_latest 返回的不是终态 Cancelled，而是“仅请求取消”的中间信号
///   - 正断言：第二次 run_next 才返回终态 Cancelled，且摘要保留“不回滚”
///   - 负断言：取消请求发出后 runner 不应继续执行 step1(FocusPrevious)
#[test]
fn test_cancel_latest_active_task_emits_cancel_requested_before_terminal_cancelled() {
    let mut queue = CardTaskQueue::default();
    let task_id = queue.enqueue(request("创建后聚焦"));
    let mut planner = FakePlanner::with_results(vec![Ok(create_then_focus_plan())]);
    let mut runner =
        FakeRunner::with_results(vec![Ok(CardCommandResult::Created { node_id: 700 })]);

    let first_events = queue.run_next(&mut planner, &mut runner);
    let cancel_event = queue
        .cancel_latest()
        .expect("active task 存在时 cancel_latest 应成功返回取消请求事件");
    let second_events = queue.run_next(&mut planner, &mut runner);

    assert!(
        first_events.iter().any(|event| matches!(event, CardTaskEvent::StepSucceeded { task_id: succeeded_id, step_index } if *succeeded_id == task_id && *step_index == 0)),
        "第一次 run_next 必须先完成 step0(create)，这个场景才是在 step boundary 前请求取消"
    );
    assert!(
        !matches!(&cancel_event, CardTaskEvent::Cancelled { .. }),
        "active task 的 cancel_latest 不应立刻返回终态 Cancelled，否则 enqueue callback 会被过早消耗"
    );
    assert!(
        second_events.iter().any(|event| matches!(event, CardTaskEvent::Cancelled { task_id: cancelled_id, message } if *cancelled_id == task_id && message.contains("not rolled back"))),
        "第二次 run_next 才应发出终态 Cancelled，并保留 completed work was not rolled back 摘要"
    );
    assert_eq!(
        runner.commands.len(),
        1,
        "CancelRequested 发出后 runner 不应继续执行 step1(FocusPrevious)"
    );
}
