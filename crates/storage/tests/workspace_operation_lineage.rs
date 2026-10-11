#[path = "support/workspace_operations.rs"]
mod support;
use agent::card_instance::{CardInstanceEvent, CardInstanceStage, CardInstanceTurn};
use storage::WorkspaceAssignmentKind;
use support::setup;

/// 安全失败只为原失败项创建新attempt，成功/Unknown和同名新卡不能重试继承任务。
/// 数据构造：A/B批次中仅A有零输入安全失败，B仍queued；新operation只含A且logical引用原任务。
/// 执行过程：接收后fail_before_input A，提交seq2的Retry，重复同op，再尝试对B伪造retry。
/// 预期结果：原逻辑任务有两attempt且seq固定，B不新增输入任务，retry资格不能靠模型声明。
#[test]
fn safe_retry_preserves_logical_task_and_has_one_stable_new_attempt() {
    let (_temp, store, request) = setup();
    store.accept(&request).expect("应接收原整组");
    store
        .fail_before_input("assignment-a", "provider_unavailable")
        .expect("A零输入安全失败");
    let mut retry = request.clone();
    retry.source.operation_id = "retry-operation".into();
    retry.source.message_id = "retry-message".into();
    retry.source.original_text = "重试原失败项".into();
    retry.assignments.truncate(1);
    retry.assignments[0].assignment.assignment_id = "retry-assignment".into();
    retry.assignments[0].task_ref = "retry-task".into();
    retry.assignments[0].attempt_sequence = 2;
    retry.assignments[0].kind = WorkspaceAssignmentKind::Retry {
        previous_assignment_id: "assignment-a".into(),
    };
    let result = store.accept(&retry).expect("确定安全的原卡重试应接收");
    assert!(result.newly_accepted, "新用户重试是新操作");
    assert_eq!(
        result.receipt.assignments[0].logical_task_ref, "task-assignment-a",
        "重试保留原canonical逻辑任务"
    );
    assert_eq!(
        result.receipt.assignments[0].attempt_sequence, 2,
        "只在接收新attempt时推进顺序"
    );
    assert!(
        !store
            .accept(&retry)
            .expect("重复重试应返回原回执")
            .newly_accepted,
        "同来源重试不得再增attempt"
    );
    retry.source.operation_id = "unsafe-retry-operation".into();
    retry.assignments[0].kind = WorkspaceAssignmentKind::Retry {
        previous_assignment_id: "assignment-b".into(),
    };
    assert!(
        store.accept(&retry).is_err(),
        "不安全或目标不同的父assignment禁止重试"
    );
    assert_eq!(
        store.assignments("alpha", "", 100).expect("完整读取").len(),
        3,
        "仅A增加一次，B仍只有原项"
    );
}

/// Reply必须关联原任务的真实turn，续聊只新增同canonical的attempt，不按新卡片状态猜父任务。
/// 数据构造：A已提交并以run/session/turn-a绑定；新reply引用A，seq2且新task_ref。
/// 执行过程：先用wrong-turn尝试，随后exact turn接收，再修改相同来源的parent/body参数。
/// 预期结果：wrong-turn零新记录，合法Reply冻结父turn、保留逻辑任务；同键改参冲突。
#[test]
fn reply_uses_exact_parent_turn_and_keeps_one_logical_task() {
    let (_temp, store, request) = setup();
    store.accept(&request).expect("应接收");
    store.arm_input("assignment-a").expect("应armed");
    let mut receipt = store
        .assignment("assignment-a")
        .expect("读A")
        .expect("A存在")
        .receipt;
    receipt.stage = CardInstanceStage::Submitted;
    store
        .record_events(&[CardInstanceEvent {
            receipt: receipt.clone(),
            observed_at_ms: 10,
            evidence: "enter_confirmed".into(),
        }])
        .expect("应有Enter证据");
    let turn = CardInstanceTurn {
        run_id: Some("run-a".into()),
        session_id: "session-a".into(),
        turn_id: "turn-a".into(),
    };
    receipt.stage = CardInstanceStage::Running;
    receipt.bound_turn = Some(turn.clone());
    store
        .record_events(&[CardInstanceEvent {
            receipt,
            observed_at_ms: 11,
            evidence: "provider_turn_started".into(),
        }])
        .expect("应绑定真实turn");
    let mut reply = request.clone();
    reply.source.operation_id = "reply-operation".into();
    reply.source.original_text = "继续原任务".into();
    reply.assignments.truncate(1);
    reply.assignments[0].assignment.assignment_id = "reply-assignment".into();
    reply.assignments[0].task_ref = "reply-task".into();
    reply.assignments[0].attempt_sequence = 2;
    let mut wrong = turn.clone();
    wrong.turn_id = "guessed-turn".into();
    reply.assignments[0].kind = WorkspaceAssignmentKind::Reply {
        parent_assignment_id: "assignment-a".into(),
        turn: wrong,
    };
    assert!(
        store.accept(&reply).is_err(),
        "模型猜测turn不能建立续聊归属"
    );
    assert_eq!(
        store.receipt(&reply.source).expect("查失败reply"),
        None,
        "wrong turn不能留下新operation"
    );
    reply.assignments[0].kind = WorkspaceAssignmentKind::Reply {
        parent_assignment_id: "assignment-a".into(),
        turn: turn.clone(),
    };
    let accepted = store.accept(&reply).expect("exact原turn续聊应接收");
    assert_eq!(
        accepted.receipt.assignments[0].logical_task_ref, "task-assignment-a",
        "续聊不能被统计成新的逻辑任务"
    );
    assert_eq!(
        accepted.receipt.assignments[0].kind,
        WorkspaceAssignmentKind::Reply {
            parent_assignment_id: "assignment-a".into(),
            turn
        },
        "父assignment和turn必须冻结"
    );
    reply.assignments[0].assignment.text = "不同回复".into();
    assert!(
        store.accept(&reply).is_err(),
        "同来源改参不得静默重用旧成功"
    );
}

/// 同一整组中的独立新任务不能共用task_ref和canonical logical引用。
/// 数据构造：原A/B稳定卡片不同，将B的新任务引用改成A的任务引用。
/// 执行过程：原子接收冲突组，再读取操作与全部assignment。
/// 预期结果：组内身份冲突全拒绝，不留下第一项或操作账本。
#[test]
fn duplicate_new_logical_references_reject_the_whole_group() {
    let (_temp, store, mut request) = setup();
    request.assignments[1].task_ref = request.assignments[0].task_ref.clone();
    request.assignments[1].logical_task_ref = request.assignments[0].logical_task_ref.clone();
    assert!(
        store.accept(&request).is_err(),
        "同组新任务不能共用canonical逻辑身份"
    );
    assert!(
        store
            .receipt(&request.source)
            .expect("应可读取拒绝后的操作")
            .is_none(),
        "拒绝不得留下操作"
    );
    assert!(
        store
            .assignment("assignment-a")
            .expect("应读取第一项")
            .is_none(),
        "拒绝不得留下首项"
    );
    assert!(
        store
            .assignment("assignment-b")
            .expect("应读取第二项")
            .is_none(),
        "拒绝不得留下第二项"
    );
}

/// 同一整组中不能为一个canonical逻辑任务创建两次相同顺序的重试。
/// 数据构造：原A零输入安全失败，复制两个seq2重试并给不同assignment/task_ref。
/// 执行过程：整组接收两个重试，再读原记录与新操作。
/// 预期结果：拒绝重复attempt，原安全失败不变，新操作与两项均不落库。
#[test]
fn duplicate_continuation_attempts_reject_the_whole_group() {
    let (_temp, store, request) = setup();
    store.accept(&request).expect("应接收原操作");
    store
        .fail_before_input("assignment-a", "not_ready")
        .expect("应证明原项零输入失败");
    let mut retry = request.clone();
    retry.source.operation_id = "duplicate-retry".into();
    retry.source.message_id = "duplicate-message".into();
    let mut first = retry.assignments[0].clone();
    first.assignment.assignment_id = "retry-a1".into();
    first.task_ref = "retry-task1".into();
    first.attempt_sequence = 2;
    first.kind = WorkspaceAssignmentKind::Retry {
        previous_assignment_id: "assignment-a".into(),
    };
    let mut second = first.clone();
    second.assignment.assignment_id = "retry-a2".into();
    second.task_ref = "retry-task2".into();
    retry.assignments = vec![first, second];
    assert!(
        store.accept(&retry).is_err(),
        "同组两个相同canonical任务的seq2不能同时接收"
    );
    assert!(
        store.receipt(&retry.source).expect("应读新操作").is_none(),
        "新重试组不能部分留下操作"
    );
    assert!(
        store.assignment("retry-a1").expect("读第一重试").is_none(),
        "第一个重试不能落库"
    );
    assert!(
        store.assignment("retry-a2").expect("读第二重试").is_none(),
        "第二个重试不能落库"
    );
    assert_eq!(
        store
            .assignment("assignment-a")
            .expect("读原项")
            .expect("原项存在")
            .receipt
            .stage,
        CardInstanceStage::Failed,
        "原安全失败必须保留"
    );
}

/// task_ref是全账本稳定attempt引用，不能被另一个logical任务的续尝试复用。
/// 数据构造：A安全失败、B原task仍存在；A重试seq2故意复用B的task_ref。
/// 执行过程：接收该重试，再查询新操作与B。
/// 预期结果：引用冲突被拒绝，B仍Accepted且没有混入A的重试历史。
#[test]
fn continuation_cannot_reuse_another_existing_task_reference() {
    let (_temp, store, request) = setup();
    store.accept(&request).expect("接收原组");
    store
        .fail_before_input("assignment-a", "not_ready")
        .expect("A零输入失败");
    let mut retry = request.clone();
    retry.source.operation_id = "task-ref-conflict".into();
    retry.source.message_id = "task-ref-message".into();
    retry.assignments.truncate(1);
    let item = &mut retry.assignments[0];
    item.assignment.assignment_id = "retry-ref".into();
    item.task_ref = "task-assignment-b".into();
    item.attempt_sequence = 2;
    item.kind = WorkspaceAssignmentKind::Retry {
        previous_assignment_id: "assignment-a".into(),
    };
    assert!(
        store.accept(&retry).is_err(),
        "已有task_ref不得被另一logical任务占用"
    );
    assert!(
        store.receipt(&retry.source).expect("读冲突操作").is_none(),
        "冲突不创建重试操作"
    );
    assert_eq!(
        store
            .assignment("assignment-b")
            .expect("读B")
            .expect("B存在")
            .receipt
            .stage,
        CardInstanceStage::Accepted,
        "B原任务阶段不变"
    );
}
