use card_task::{
    execute_card_task_step, CardCommandRunner, CardTaskExecutionContext, CardTaskExecutionError,
    CardTaskStep,
};
use norma::protocol::{CardCommand, CardCommandError, CardCommandResult, CardKind, Placement};

#[derive(Default)]
struct FakeRunner {
    commands: Vec<CardCommand>,
}

impl CardCommandRunner for FakeRunner {
    fn run(&mut self, command: CardCommand) -> Result<CardCommandResult, CardCommandError> {
        let result = match &command {
            CardCommand::Create { .. } => CardCommandResult::Created { node_id: 41 },
            CardCommand::SendText { node_id, .. } => CardCommandResult::Sent { node_id: *node_id },
            CardCommand::Focus { node_id } => CardCommandResult::Focused { node_id: *node_id },
            CardCommand::Delete { node_id } => CardCommandResult::Deleted { node_id: *node_id },
            CardCommand::List => CardCommandResult::Listed { cards: Vec::new() },
            CardCommand::ListAgents
            | CardCommand::CreateAgent { .. }
            | CardCommand::ActivateAgent { .. }
            | CardCommand::DispatchAgentTasks { .. } => {
                return Err(CardCommandError::Unsupported {
                    message: "legacy card_task runner does not execute Agent coordination".into(),
                });
            }
        };
        self.commands.push(command);
        Ok(result)
    }
}

/// E-1: previous-step create 产出的 node_id 应被后续 SendText/Focus 正确解析，并保留 submit 标记。
///
/// 数据构造（含关键数值的推导过程）：
///   total_steps = 4
///   step0 = Create Terminal，runner 返回 node_id = 41
///   step1 = SendTextToPrevious(step0, submit=false)
///   step2 = SendTextToPrevious(step0, submit=true)
///   step3 = FocusPrevious(step0)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 依次执行 step0/1/2/3
///   2. step0 记录 created_by_step[0] = 41
///   3. step1/2/3 从 execution context 解析 node_id=41，再下发给 runner
///
/// 预期结果：
///   - 正断言：第二条命令是 SendText(node_id=41, submit=false)
///   - 正断言：第三条命令是 SendText(node_id=41, submit=true)
///   - 正断言：第四条命令是 Focus(node_id=41)
#[test]
fn test_execute_card_task_step_resolves_previous_created_node_and_preserves_submit_flags() {
    let steps = vec![
        CardTaskStep::Command(CardCommand::Create {
            kind: CardKind::Terminal { shell: None },
            placement: Placement::VisibleArea,
        }),
        CardTaskStep::SendTextToPrevious {
            previous_step: 0,
            text: "draft".to_string(),
            submit: false,
        },
        CardTaskStep::SendTextToPrevious {
            previous_step: 0,
            text: "run".to_string(),
            submit: true,
        },
        CardTaskStep::FocusPrevious { previous_step: 0 },
    ];
    let mut context = CardTaskExecutionContext::default();
    let mut runner = FakeRunner::default();

    execute_card_task_step(0, &steps[0], steps.len(), &mut context, &mut runner)
        .expect("step0 create 应成功并记录 created_by_step");
    execute_card_task_step(1, &steps[1], steps.len(), &mut context, &mut runner)
        .expect("step1 send_text submit=false 应成功解析 previous step");
    execute_card_task_step(2, &steps[2], steps.len(), &mut context, &mut runner)
        .expect("step2 send_text submit=true 应成功解析 previous step");
    execute_card_task_step(3, &steps[3], steps.len(), &mut context, &mut runner)
        .expect("step3 focus 应成功解析 previous step");

    assert_eq!(
        runner.commands[1],
        CardCommand::SendText {
            node_id: 41,
            text: "draft".to_string(),
            submit: false,
        },
        "submit=false 必须原样透传到 CardCommand::SendText"
    );
    assert_eq!(
        runner.commands[2],
        CardCommand::SendText {
            node_id: 41,
            text: "run".to_string(),
            submit: true,
        },
        "submit=true 也必须原样透传到 CardCommand::SendText"
    );
    assert_eq!(
        runner.commands[3],
        CardCommand::Focus { node_id: 41 },
        "FocusPrevious 应解析到 step0 创建出来的 node_id=41"
    );
}

/// E-2: previous_step 指向不存在的更早步骤时，应返回 MissingPreviousStep。
///
/// 数据构造（含关键数值的推导过程）：
///   total_steps = 1
///   current_step = 0
///   previous_step = 3，因此 3 >= total_steps 且 3 >= current_step
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造一个单步 SendTextToPrevious(step=3)
///   2. 在空 execution context 上执行
///
/// 预期结果：
///   - 正断言：返回 MissingPreviousStep { step: 3 }
#[test]
fn test_execute_card_task_step_fails_when_previous_step_is_missing() {
    let step = CardTaskStep::SendTextToPrevious {
        previous_step: 3,
        text: "hello".to_string(),
        submit: false,
    };
    let mut context = CardTaskExecutionContext::default();
    let mut runner = FakeRunner::default();

    let error = execute_card_task_step(0, &step, 1, &mut context, &mut runner)
        .expect_err("引用不存在 previous step 的 step 必须失败");

    assert_eq!(
        error,
        CardTaskExecutionError::MissingPreviousStep { step: 3 },
        "不存在的 previous step 应返回 MissingPreviousStep(step=3)"
    );
}

/// E-3: previous_step 存在但未创建 node 时，应返回 PreviousStepDidNotCreateNode。
///
/// 数据构造（含关键数值的推导过程）：
///   total_steps = 2
///   step0 = Focus(node_id=9)，返回 Focused，不会写入 created_by_step
///   step1 = SendTextToPrevious(step0)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 执行 step0，context.created_by_step 仍为空
///   2. 执行 step1，解析 previous_step=0 但找不到创建记录
///
/// 预期结果：
///   - 正断言：step1 返回 PreviousStepDidNotCreateNode { step: 0 }
#[test]
fn test_execute_card_task_step_fails_when_previous_step_did_not_create_node() {
    let steps = vec![
        CardTaskStep::Command(CardCommand::Focus { node_id: 9 }),
        CardTaskStep::SendTextToPrevious {
            previous_step: 0,
            text: "hello".to_string(),
            submit: false,
        },
    ];
    let mut context = CardTaskExecutionContext::default();
    let mut runner = FakeRunner::default();

    execute_card_task_step(0, &steps[0], steps.len(), &mut context, &mut runner)
        .expect("step0 focus 作为非创建命令本身应成功");
    let error = execute_card_task_step(1, &steps[1], steps.len(), &mut context, &mut runner)
        .expect_err("引用未创建 node 的 previous step 必须失败");

    assert_eq!(
        error,
        CardTaskExecutionError::PreviousStepDidNotCreateNode { step: 0 },
        "存在但未创建 node 的 previous step 应返回 PreviousStepDidNotCreateNode(step=0)"
    );
}

/// E-4: 白名单中的 List 与 Delete 命令应能作为普通 CardCommand 透传给 runner。
///
/// 数据构造（含关键数值的推导过程）：
///   total_steps = 2
///   step0 = List
///   step1 = Delete(node_id=88)
///   fake runner 对 List 返回 Listed，对 Delete 返回 Deleted(88)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 顺序执行 step0(List) 与 step1(Delete)
///   2. 记录 fake runner 接收到的原始命令顺序
///
/// 预期结果：
///   - 正断言：step0 返回 Listed
///   - 正断言：step1 返回 Deleted(node_id=88)
///   - 正断言：runner 第 1 条命令是 List
///   - 正断言：runner 第 2 条命令是 Delete(node_id=88)
#[test]
fn test_execute_card_task_step_supports_list_and_delete_commands() {
    let steps = [
        CardTaskStep::Command(CardCommand::List),
        CardTaskStep::Command(CardCommand::Delete { node_id: 88 }),
    ];
    let mut context = CardTaskExecutionContext::default();
    let mut runner = FakeRunner::default();

    let list_result = execute_card_task_step(0, &steps[0], steps.len(), &mut context, &mut runner)
        .expect("List 命令应能被 fake runner 正常执行");
    let delete_result =
        execute_card_task_step(1, &steps[1], steps.len(), &mut context, &mut runner)
            .expect("Delete 命令应能被 fake runner 正常执行");

    assert_eq!(
        list_result,
        CardCommandResult::Listed { cards: Vec::new() },
        "List 命令必须返回 Listed(empty)"
    );
    assert_eq!(
        delete_result,
        CardCommandResult::Deleted { node_id: 88 },
        "Delete(node_id=88) 必须返回 Deleted(node_id=88)"
    );
    assert_eq!(
        runner.commands[0],
        CardCommand::List,
        "runner 收到的第一条命令必须保持为 CardCommand::List"
    );
    assert_eq!(
        runner.commands[1],
        CardCommand::Delete { node_id: 88 },
        "runner 收到的第二条命令必须保持为 CardCommand::Delete(node_id=88)"
    );
}
