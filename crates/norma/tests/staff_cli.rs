use norma::cli::parse_command_from;
use norma::protocol::{BrainOp, CardCommand, StaffOp};

/// 新 assign 的 task 是任务 ID，旧 assign 的 task 仍是指令；提议先于采用。
///
/// 数据构造：task-a，proposal=1（首条提议），旧指令为 implement。
/// 执行过程：解析新分派、旧分派、任务提议三个公开 CLI 调用。
/// 预期结果：新调用产生 Adopt，旧调用产生 Assign，提议不能直接产生 Worker 分派。
#[test]
fn staff_assignment_preserves_legacy_task_text() {
    let staff = parse_command_from([
        "norma",
        "brain",
        "assign",
        "--task",
        "task-a",
        "--proposal",
        "1",
    ])
    .expect("staff assignment");
    let legacy = parse_command_from(["norma", "brain", "assign", "Simon", "--task", "implement"])
        .expect("legacy assignment");
    assert!(
        matches!(staff.brain_op(), Some(BrainOp::Task { op: StaffOp::Adopt { task_id, proposal_id: 1 } }) if task_id == "task-a"),
        "new --task must identify a staff task"
    );
    assert!(
        matches!(legacy.brain_op(), Some(BrainOp::Assign { agent, task }) if agent == "Simon" && task == "implement"),
        "legacy instruction must remain intact"
    );
    let proposed = parse_command_from([
        "norma",
        "brain",
        "task",
        "propose",
        "task-a",
        "Simon",
        "--title",
        "Implement",
        "--prompt",
        "implement and test",
        "--why",
        "Required",
    ])
    .expect("proposal");
    assert!(
        matches!(proposed, CardCommand::Brain { op, .. } if matches!(*op, BrainOp::Task { op: StaffOp::Propose { .. } })),
        "proposal must remain a proposal, never an assignment"
    );
    assert!(
        parse_command_from(["norma", "brain", "assign", "--task", "task-a"]).is_err(),
        "missing legacy agent or staff proposal must be rejected"
    );
}

/// 模型不得通过 CLI 冒充用户发消息；用户消息只走面板内部路径。
///
/// 数据构造：`norma brain task message task-a --text x`。
/// 预期结果：解析失败，不产生 StaffOp::Message。
#[test]
fn model_cli_cannot_send_user_messages() {
    assert!(
        parse_command_from(["norma", "brain", "task", "message", "task-a", "--text", "x"]).is_err(),
        "brain task message must not exist for the model"
    );
}

/// 幕僚自己派活走 `task assign`，必须带任务 ID、Agent、标题、完整指令和理由。
///
/// 数据构造：task-a、Simon，标题 Implement，指令 implement and test，理由 Required。
/// 执行过程：解析 `norma brain task assign`，再解析缺 `--why` 的调用。
/// 预期结果：产生 StaffOp::Assign 并保留全部字段；缺字段被拒绝。
#[test]
fn staff_can_assign_follow_up_work_to_an_agent() {
    let assigned = parse_command_from([
        "norma",
        "brain",
        "task",
        "assign",
        "task-a",
        "Simon",
        "--title",
        "Implement",
        "--prompt",
        "implement and test",
        "--why",
        "Required",
    ])
    .expect("task assign");
    assert!(
        matches!(
            assigned.brain_op(),
            Some(BrainOp::Task { op: StaffOp::Assign { task_id, agent, title, prompt, why } })
                if task_id == "task-a" && agent == "Simon" && title == "Implement"
                    && prompt == "implement and test" && why == "Required"
        ),
        "task assign must carry every field"
    );
    assert!(
        parse_command_from([
            "norma", "brain", "task", "assign", "task-a", "Simon", "--title", "T", "--prompt", "P",
        ])
        .is_err(),
        "assign without --why must be rejected"
    );
}

/// 幕僚可以归档任务，但只能归档，不能取消归档。
///
/// 数据构造：`norma brain task archive task-a`。
/// 预期结果：产生 archived=true 的 StaffOp::Archive；带 --undo 之类的取消归档参数被拒绝。
#[test]
fn staff_can_archive_but_not_unarchive_a_task() {
    let archived =
        parse_command_from(["norma", "brain", "task", "archive", "task-a"]).expect("task archive");
    assert!(
        matches!(
            archived.brain_op(),
            Some(BrainOp::Task { op: StaffOp::Archive { task_id, archived: true } }) if task_id == "task-a"
        ),
        "task archive must archive the named task"
    );
    assert!(
        parse_command_from(["norma", "brain", "task", "archive", "task-a", "--undo"]).is_err(),
        "the model must not be able to unarchive"
    );
}
