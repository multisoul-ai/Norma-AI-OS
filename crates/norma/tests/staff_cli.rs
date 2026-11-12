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
