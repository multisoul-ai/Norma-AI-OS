use card_task::{
    CardCommandRunner, CardTaskCancelError, CardTaskEvent, CardTaskPlanner, CardTaskPlannerError,
    CardTaskQueue, CardTaskRequest, CardTaskState,
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
}

impl CardCommandRunner for FakeRunner {
    fn run(&mut self, command: CardCommand) -> Result<CardCommandResult, CardCommandError> {
        let result = match &command {
            CardCommand::Create { .. } => CardCommandResult::Created {
                node_id: self.commands.len() as u64 + 100,
            },
            CardCommand::Focus { node_id } => CardCommandResult::Focused { node_id: *node_id },
            CardCommand::SendText { node_id, .. } => CardCommandResult::Sent { node_id: *node_id },
            CardCommand::Delete { node_id } => CardCommandResult::Deleted { node_id: *node_id },
            CardCommand::List => CardCommandResult::Listed { cards: Vec::new() },
        };
        self.commands.push(command);
        Ok(result)
    }
}

fn request(intent: &str) -> CardTaskRequest {
    CardTaskRequest {
        intent: intent.to_string(),
        tool_call_id: None,
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

/// Q-1: enqueue_card_task(intent) 应创建 queued task，并发出 Queued 事件。
///
/// 数据构造（含关键数值的推导过程）：
///   pending tasks 初始 = 0
///   enqueue requests = 1
///   tool_call_id = None，验证最小请求形态也能入队
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建空队列
///   2. enqueue("创建一个 Codex 卡片")
///   3. drain_events 读取同步发出的排队事件
///
/// 预期结果：
///   - 正断言：pending_len 从 0 变成 1
///   - 正断言：唯一事件是 Queued，且 task_id 与 enqueue 返回值一致
///   - 正断言：首个 pending task 处于 Queued 状态
#[test]
fn test_enqueue_creates_one_queued_task_and_emits_queued_event() {
    let mut queue = CardTaskQueue::default();

    let task_id = queue.enqueue(request("创建一个 Codex 卡片"));
    let events = queue.drain_events();

    assert_eq!(
        queue.pending_len(),
        1,
        "enqueue 后 pending_len 应从 0 变成 1"
    );
    assert_eq!(events.len(), 1, "enqueue 只应同步发出一个 Queued 事件");
    assert_eq!(
        events[0],
        CardTaskEvent::Queued {
            task_id,
            summary: "创建一个 Codex 卡片".to_string(),
        },
        "Queued 事件应带回相同 task_id 和默认 summary"
    );
    assert_eq!(
        queue.pending_tasks()[0].state,
        CardTaskState::Queued,
        "新入队 task 必须保持 Queued 状态，不能提前进入 Planning"
    );
}

/// Q-2: 串行队列任意时刻最多一个 active task，第二个 task 需等第一个完成后再执行。
///
/// 数据构造（含关键数值的推导过程）：
///   task A steps = 2（create + focus previous）
///   task B steps = 2（create + focus previous）
///   pending 初始 = 2，因此执行 task A 的第一步后 task B 仍应保留在 pending
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 依次 enqueue task A / task B
///   2. 第一次 run_next()：task A 规划并执行 step0(create)，保留为 active
///   3. 第二次 run_next()：task A 执行 step1(focus) 并完成
///   4. 第三次 run_next()：task B 才允许开始执行
///
/// 预期结果：
///   - 正断言：第一次 run_next 后 active_task 是 task A，pending_len 仍为 1
///   - 正断言：第三次 run_next 才开始 task B
///   - 负断言：第一次 run_next 后 task B 不应变成 active
#[test]
fn test_serial_queue_keeps_second_task_queued_until_first_finishes() {
    let mut queue = CardTaskQueue::default();
    let first_id = queue.enqueue(request("先创建再聚焦 A"));
    let second_id = queue.enqueue(request("先创建再聚焦 B"));
    let mut planner = FakePlanner::with_results(vec![
        Ok(create_then_focus_plan()),
        Ok(create_then_focus_plan()),
    ]);
    let mut runner = FakeRunner::default();

    let first_events = queue.run_next(&mut planner, &mut runner);

    assert_eq!(
        queue.pending_len(),
        1,
        "第一次 run_next 后第二个 task 应继续留在 pending"
    );
    assert_eq!(
        queue.active_task().map(|task| task.task_id),
        Some(first_id),
        "第一次 run_next 后 active_task 必须仍是第一个 task"
    );
    assert_ne!(
        queue.active_task().map(|task| task.task_id),
        Some(second_id),
        "第一次 run_next 后第二个 task 绝不能抢占成 active"
    );
    assert!(
        first_events.iter().any(|event| matches!(event, CardTaskEvent::Executing { task_id, step_index } if *task_id == first_id && *step_index == 0)),
        "第一次 run_next 应执行第一个 task 的 step0"
    );

    let second_events = queue.run_next(&mut planner, &mut runner);
    let third_events = queue.run_next(&mut planner, &mut runner);

    assert!(
        second_events.iter().any(|event| matches!(event, CardTaskEvent::Succeeded { task_id, .. } if *task_id == first_id)),
        "第二次 run_next 应让第一个 task 进入 Succeeded"
    );
    assert!(
        third_events.iter().any(
            |event| matches!(event, CardTaskEvent::Planning { task_id } if *task_id == second_id)
        ),
        "第三次 run_next 才应开始第二个 task 的 Planning"
    );
}

/// Q-3: planner provider error 应让 task 终止为 Failed。
///
/// 数据构造（含关键数值的推导过程）：
///   planner result count = 1
///   provider error message len = 13（\"provider down\"）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. enqueue 单个 task
///   2. planner 返回 Provider 错误
///   3. run_next() 收集 Planning -> Failed 事件
///
/// 预期结果：
///   - 正断言：事件序列包含 Failed(provider down)
///   - 正断言：active_task 最终为空
#[test]
fn test_planner_provider_error_emits_failed() {
    let mut queue = CardTaskQueue::default();
    let task_id = queue.enqueue(request("planner provider error"));
    let mut planner = FakePlanner::with_results(vec![Err(CardTaskPlannerError::Provider {
        message: "provider down".to_string(),
    })]);
    let mut runner = FakeRunner::default();

    let events = queue.run_next(&mut planner, &mut runner);

    assert!(
        events.iter().any(|event| matches!(event, CardTaskEvent::Failed { task_id: failed_id, message } if *failed_id == task_id && message.contains("provider down"))),
        "planner provider 错误必须向外发出 Failed(provider down)"
    );
    assert!(
        queue.active_task().is_none(),
        "planner 失败后 active_task 必须被清空"
    );
}

/// Q-4: malformed/compile planner error 应让 task 终止为 Failed。
///
/// 数据构造（含关键数值的推导过程）：
///   planner result count = 1
///   compile error message len = 14（\"malformed plan\"）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. enqueue 单个 task
///   2. planner 返回 Compile 错误
///   3. run_next() 收集 Planning -> Failed 事件
///
/// 预期结果：
///   - 正断言：事件序列包含 Failed(malformed plan)
///   - 正断言：runner 不应收到任何命令
#[test]
fn test_compile_error_emits_failed() {
    let mut queue = CardTaskQueue::default();
    let task_id = queue.enqueue(request("planner compile error"));
    let mut planner = FakePlanner::with_results(vec![Err(CardTaskPlannerError::Compile {
        message: "malformed plan".to_string(),
    })]);
    let mut runner = FakeRunner::default();

    let events = queue.run_next(&mut planner, &mut runner);

    assert!(
        events.iter().any(|event| matches!(event, CardTaskEvent::Failed { task_id: failed_id, message } if *failed_id == task_id && message.contains("malformed plan"))),
        "compile 错误必须被映射成 Failed(malformed plan)"
    );
    assert_eq!(
        runner.commands.len(),
        0,
        "compile 失败时 runner 不应执行任何 CardCommand"
    );
}

/// Q-5: queued task 取消应移除最新 queued task，并发出 Cancelled。
///
/// 数据构造（含关键数值的推导过程）：
///   pending tasks = 2
///   cancel scope   = latest，因此应移除第二个 task
///
/// 执行过程（逐步说明系统如何处理）：
///   1. enqueue task A / task B
///   2. cancel_latest()
///   3. 检查 pending 队列只剩 task A
///
/// 预期结果：
///   - 正断言：返回事件为 task B 的 Cancelled
///   - 正断言：pending_len 从 2 变成 1
///   - 负断言：task A 不应被错误取消
#[test]
fn test_cancel_latest_removes_newest_queued_task() {
    let mut queue = CardTaskQueue::default();
    let first_id = queue.enqueue(request("task A"));
    let second_id = queue.enqueue(request("task B"));

    let event = queue
        .cancel_latest()
        .expect("存在 queued task 时 cancel_latest 应成功");

    assert_eq!(
        event,
        CardTaskEvent::Cancelled {
            task_id: second_id,
            message: "cancelled queued task".to_string(),
        },
        "latest queued task 应被直接取消，并返回对应的 Cancelled 事件"
    );
    assert_eq!(
        queue.pending_len(),
        1,
        "取消最新 queued task 后 pending_len 应从 2 变成 1"
    );
    assert_eq!(
        queue.pending_tasks()[0].task_id,
        first_id,
        "保留下来的 pending task 应该仍是最早入队的 task A"
    );
}

/// Q-6: active task 取消应在 step 边界停止，不回滚已完成 step。
///
/// 数据构造（含关键数值的推导过程）：
///   task A steps = 2（create + focus previous）
///   task B steps = 2（create + focus previous），在 A 取消后再入队，用于验证队列继续前进
///   create result node_id = 100（FakeRunner 第 1 条命令固定返回）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. enqueue task A
///   2. 第一次 run_next() 执行 task A 的 step0(create)，A 仍为 active
///   3. cancel_latest() 对 active task A 发起取消
///   4. 再 enqueue task B
///   5. 下一次 run_next() 开始 task B
///
/// 预期结果：
///   - 正断言：cancel_latest 返回 task A 的 Cancelled
///   - 正断言：runner 只执行了 task A 的 step0(create)，没有执行 step1(focus)
///   - 正断言：task A 取消后，下一次 run_next() 才开始 task B
#[test]
fn test_cancel_active_task_stops_before_next_step_without_rollback() {
    let mut queue = CardTaskQueue::default();
    let first_id = queue.enqueue(request("task A"));
    let mut planner = FakePlanner::with_results(vec![
        Ok(create_then_focus_plan()),
        Ok(create_then_focus_plan()),
    ]);
    let mut runner = FakeRunner::default();

    let first_events = queue.run_next(&mut planner, &mut runner);
    let cancel_event = queue
        .cancel_latest()
        .expect("存在 active task 时 cancel_latest 应成功");

    assert!(
        first_events.iter().any(|event| matches!(event, CardTaskEvent::StepSucceeded { task_id, step_index } if *task_id == first_id && *step_index == 0)),
        "第一次 run_next 应先完成 task A 的 step0(create)"
    );
    assert_eq!(
        cancel_event,
        CardTaskEvent::Cancelled {
            task_id: first_id,
            message: "cancelled active task".to_string(),
        },
        "active task 取消应返回 task A 的 Cancelled 事件"
    );
    assert_eq!(
        runner.commands.len(),
        1,
        "取消 task A 后，在 task B 开始前 runner 只应保留已完成的 step0"
    );
    assert!(
        matches!(&runner.commands[0], CardCommand::Create { .. }),
        "已完成且保留下来的唯一副作用应是 step0 的 Create 命令"
    );

    let second_id = queue.enqueue(request("task B"));
    let third_events = queue.run_next(&mut planner, &mut runner);

    assert!(
        third_events.iter().any(
            |event| matches!(event, CardTaskEvent::Planning { task_id } if *task_id == second_id)
        ),
        "task A 取消后，下一次 run_next 才应开始 task B"
    );
}

/// Q-7: 没有 pending 或 active task 时取消应返回 no active card task。
///
/// 数据构造（含关键数值的推导过程）：
///   pending = 0
///   active  = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建空队列
///   2. 直接调用 cancel_latest()
///
/// 预期结果：
///   - 正断言：返回 CardTaskCancelError::NoActiveCardTask
#[test]
fn test_cancel_latest_without_tasks_returns_no_active_error() {
    let mut queue = CardTaskQueue::default();

    let error = queue
        .cancel_latest()
        .expect_err("空队列调用 cancel_latest 必须返回错误");

    assert_eq!(
        error,
        CardTaskCancelError::NoActiveCardTask,
        "没有 pending/active task 时必须返回 NoActiveCardTask"
    );
}
