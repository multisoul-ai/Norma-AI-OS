use card_task::{
    CardCommandRunner, CardTaskEvent, CardTaskPlanner, CardTaskPlannerError, CardTaskQueue,
    CardTaskRequest, FlatCardTaskRequest,
};
use norma::protocol::{CardCommand, CardCommandError, CardCommandResult, CardKind, Placement};
use std::collections::VecDeque;
use voice::CompiledVoicePlan;

/// 扁平入队路径不应调用 planner；若被调用则测试失败。
struct NoopPlanner {
    call_count: usize,
}

impl NoopPlanner {
    fn new() -> Self {
        Self { call_count: 0 }
    }
}

impl CardTaskPlanner for NoopPlanner {
    fn plan(
        &mut self,
        _request: &CardTaskRequest,
    ) -> Result<CompiledVoicePlan, CardTaskPlannerError> {
        self.call_count += 1;
        panic!("flat enqueue 不应调用 planner");
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
        self.commands.push(command);
        if let Some(result) = self.results.pop_front() {
            return result;
        }

        match &self.commands[self.commands.len() - 1] {
            CardCommand::Create { .. } => Ok(CardCommandResult::Created { node_id: 7 }),
            CardCommand::SendText { node_id, .. } => {
                Ok(CardCommandResult::Sent { node_id: *node_id })
            }
            CardCommand::Focus { node_id } => Ok(CardCommandResult::Focused { node_id: *node_id }),
            CardCommand::Delete { node_id } => Ok(CardCommandResult::Deleted { node_id: *node_id }),
            CardCommand::List => Ok(CardCommandResult::Listed { cards: Vec::new() }),
        }
    }
}

/// QF-1: enqueue_flat 应跳过 planner，按顺序执行已解析的 CardCommand 步骤。
///
/// 数据构造：
///   steps = [Create Terminal, SendText{node_id:7, text:"hi", submit:true}]
///   FakeRunner 对两步均返回成功
///   NoopPlanner 若被调用会 panic
///
/// 执行过程：
///   1. enqueue_flat 入队两步任务
///   2. run_next 第一次：应执行 Create
///   3. run_next 第二次：应执行 SendText 并完成任务
///
/// 预期结果：
///   - 正断言：runner.commands 长度为 2，且顺序为 Create 再 SendText(node_id=7)
///   - 正断言：第二次 run_next 后任务进入 Succeeded
///   - 负断言：planner.call_count 始终为 0
///   - 负断言：事件流中不出现 Planning
#[test]
fn test_enqueue_flat_executes_resolved_steps_without_planner() {
    let mut queue = CardTaskQueue::default();
    let mut planner = NoopPlanner::new();
    let mut runner = FakeRunner::with_results(vec![
        Ok(CardCommandResult::Created { node_id: 7 }),
        Ok(CardCommandResult::Sent { node_id: 7 }),
    ]);

    let task_id = queue.enqueue_flat(FlatCardTaskRequest {
        summary: "创建终端并发送文本".to_string(),
        steps: vec![
            CardCommand::Create {
                kind: CardKind::Terminal { shell: None },
                placement: Placement::VisibleArea,
            },
            CardCommand::SendText {
                node_id: 7,
                text: "hi".to_string(),
                submit: true,
            },
        ],
        tool_call_id: Some("call_flat".to_string()),
    });

    assert_eq!(
        queue.pending_len(),
        1,
        "enqueue_flat 后 pending 队列应有 1 个任务"
    );

    let first_events = queue.run_next(&mut planner, &mut runner);
    assert_eq!(planner.call_count, 0, "第一次 run_next 不应触发 planner");
    assert!(
        !first_events
            .iter()
            .any(|event| matches!(event, CardTaskEvent::Planning { .. })),
        "扁平任务事件流不应包含 Planning"
    );
    assert!(
        first_events
            .iter()
            .any(|event| matches!(event, CardTaskEvent::Planned { task_id: id, step_count: 2 } if *id == task_id)),
        "第一次 run_next 应发出 Planned(step_count=2)"
    );
    assert_eq!(runner.commands.len(), 1, "第一次 run_next 应执行 Create");
    assert!(
        matches!(
            &runner.commands[0],
            CardCommand::Create {
                kind: CardKind::Terminal { shell: None },
                ..
            }
        ),
        "第一步应为 Create Terminal"
    );

    let second_events = queue.run_next(&mut planner, &mut runner);
    assert_eq!(planner.call_count, 0, "第二次 run_next 仍不应触发 planner");
    assert_eq!(runner.commands.len(), 2, "第二次 run_next 应执行 SendText");
    assert!(
        matches!(
            &runner.commands[1],
            CardCommand::SendText {
                node_id: 7,
                text,
                submit: true,
            } if text == "hi"
        ),
        "第二步应为 SendText(node_id=7)"
    );
    assert!(
        second_events.iter().any(
            |event| matches!(event, CardTaskEvent::Succeeded { task_id: id, .. } if *id == task_id)
        ),
        "两步执行完成后任务应 Succeeded"
    );
    assert_eq!(
        queue.completed_tasks().len(),
        1,
        "完成后 completed 队列应有 1 个任务"
    );
}
