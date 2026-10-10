#[path = "support/card_instance_assignment.rs"]
mod support;
use agent::card_instance::{CardInstanceCoordinator, CardInstanceStage};
use support::assignment;

/// 同一配置的两张卡片与无定义卡片必须各自接受任务，不按 definition 复用首卡。
/// 数据构造：node 11/12 共用 definition 7；node 13 无定义，共 3 张、3 项任务。
/// 执行过程：以冻结 target 接收三项，通过稳定 assignment 引用读取回执和待发送任务。
/// 预期结果：回执分别保留 11/12/13 的完整身份；读取第二项不得返回第一张卡片。
#[test]
fn tasks_are_bound_to_card_instances_including_undefined_cards() {
    let mut coordinator = CardInstanceCoordinator::default();
    for assignment in [
        assignment("first", 11, "card-first", Some(7)),
        assignment("second", 12, "card-second", Some(7)),
        assignment("plain", 13, "card-plain", None),
    ] {
        coordinator.accept(assignment).expect("合法实例应接收");
    }
    let second = coordinator.receipt("second").expect("第二项存在");
    assert_eq!(
        second.target.card.node_id, 12,
        "共享定义不能把任务投往首卡 11"
    );
    assert_eq!(
        second.target.card.card_instance_id, "card-second",
        "持久身份必须保留"
    );
    assert_eq!(
        second.stage,
        CardInstanceStage::Accepted,
        "接收不能宣称已提交或运行"
    );
    assert_ne!(
        second.target.card.node_id, 11,
        "不能按 definition 选择第一张卡片"
    );
    assert_eq!(
        coordinator
            .receipt("plain")
            .expect("无定义任务")
            .target
            .card
            .node_id,
        13,
        "无定义卡片也可接收"
    );
    assert_eq!(
        coordinator.pending_assignments().len(),
        3,
        "三个实例分别保留待发送项"
    );
}

/// B 的传输可越过 A 的终态等待，但 B 只能由自身 Provider turn 开始和完成。
/// 数据构造：同卡 first/second 共 2 项，first-turn/second-turn 各 1 个可信 Provider 标识。
/// 执行过程：接收 A/B，A Enter+start，A 尚运行时 B Enter，依次上报 A 终态、B start/终态。
/// 预期结果：A 完成后 B 仍 Submitted；A 的迟到/重复终态不完成 B；B 独立完成且保留原引用。
#[test]
fn delivery_does_not_wait_for_completion_or_invent_followup_execution() {
    use agent::card_instance::CardInstanceTurn;
    let mut coordinator = CardInstanceCoordinator::default();
    let first = assignment("first", 11, "card", Some(7));
    let second = assignment("second", 11, "card", Some(7));
    let target = first.target.clone();
    let first_turn = CardInstanceTurn {
        run_id: Some("run".into()),
        session_id: "session".into(),
        turn_id: "first-turn".into(),
    };
    let second_turn = CardInstanceTurn {
        turn_id: "second-turn".into(),
        ..first_turn.clone()
    };
    coordinator.accept(first).expect("A 接收");
    coordinator.accept(second).expect("B 接收");
    coordinator.submitted("first").expect("A Enter");
    assert!(
        coordinator.observe_start(&target, "task-first", first_turn.clone()),
        "A 自身 start 应绑定"
    );
    assert_eq!(
        coordinator.pending_assignments()[0].assignment_id,
        "second",
        "A 未结束仍可传输 B"
    );
    coordinator.submitted("second").expect("B Enter");
    assert_eq!(
        coordinator.receipt("second").expect("B").stage,
        CardInstanceStage::Submitted,
        "B 不继承卡片 A 的运行状态"
    );
    assert!(
        coordinator.observe_terminal(&target, &first_turn, CardInstanceStage::Succeeded, None),
        "仅 A 终态应接受"
    );
    assert_eq!(
        coordinator.receipt("first").expect("A").stage,
        CardInstanceStage::Succeeded,
        "A 独立结束"
    );
    assert_eq!(
        coordinator.receipt("second").expect("B").stage,
        CardInstanceStage::Submitted,
        "A 完成不能提前运行 B"
    );
    assert!(
        !coordinator.observe_terminal(&target, &first_turn, CardInstanceStage::Succeeded, None),
        "重复 A 终态不得投向 B"
    );
    assert!(
        coordinator.observe_start(&target, "task-second", second_turn.clone()),
        "B 自身 start 才运行"
    );
    assert!(
        coordinator.observe_terminal(&target, &second_turn, CardInstanceStage::Succeeded, None),
        "B 自身终态才完成"
    );
    assert!(
        coordinator.pending_assignments().is_empty(),
        "已 Enter 的两项都不能重发"
    );
    assert_eq!(
        coordinator.receipt("second").expect("B").bound_turn,
        Some(second_turn),
        "后续取消可读取精确 run/session/turn"
    );
}

/// 删除重建和改参不能继承原 assignment，旧 run/session 证据也不能完成任务。
/// 数据构造：1 项原 assignment、相同 ID 的变参副本、新持久 card ID、不同 session 的 turn。
/// 执行过程：接收原任务，重复相同请求，再提交变参请求和错误实例/会话的 start。
/// 预期结果：相同请求不重复排队；变参 conflict；所有错误证据零状态提升。
#[test]
fn idempotency_and_evidence_keep_original_card_and_session() {
    use agent::card_instance::{CardInstanceError, CardInstanceTurn};
    let mut coordinator = CardInstanceCoordinator::default();
    let original = assignment("first", 11, "card-original", Some(7));
    coordinator.accept(original.clone()).expect("原任务");
    coordinator
        .accept(original.clone())
        .expect("相同请求应返回原回执");
    let mut changed = original.clone();
    changed.text = "另一个正文".into();
    assert_eq!(
        coordinator.accept(changed),
        Err(CardInstanceError::Conflict),
        "同引用变参必须冲突"
    );
    assert_eq!(
        coordinator.pending_assignments().len(),
        1,
        "重发不能扩大队列"
    );
    coordinator.submitted("first").expect("Enter");
    let turn = CardInstanceTurn {
        run_id: Some("run".into()),
        session_id: "session".into(),
        turn_id: "turn".into(),
    };
    let mut replacement = original.target.clone();
    replacement.card.card_instance_id = "new-card".into();
    assert!(
        !coordinator.observe_start(&replacement, "task-first", turn.clone()),
        "同 node 新卡不得继承证据"
    );
    let wrong = CardInstanceTurn {
        session_id: "old-session".into(),
        ..turn
    };
    assert!(
        !coordinator.observe_start(&original.target, "task-first", wrong),
        "错误 session 不得绑定运行"
    );
    assert_eq!(
        coordinator.receipt("first").expect("任务").stage,
        CardInstanceStage::Submitted,
        "不能根据错误 start 推断运行"
    );
    assert_eq!(
        coordinator.receipt("first").expect("任务").bound_turn,
        None,
        "不能冻结错误 turn"
    );
}

/// Enter 已确认后，调用方的零写入声明不能倒置事实或开放盲目安全重试。
/// 数据构造：1 项 Submitted 任务，故障上报误带 zero_input_proven=true。
/// 执行过程：accept → submitted → fail；从公开 receipt 读取最终状态和重试资格。
/// 预期结果：仍 OutcomeUnknown、retry_safe=false，不能伪装正文未交给 CLI。
#[test]
fn submitted_fact_cannot_be_overwritten_by_a_zero_input_claim() {
    let mut coordinator = CardInstanceCoordinator::default();
    coordinator
        .accept(assignment("first", 11, "card", None))
        .expect("接收");
    coordinator.submitted("first").expect("已 Enter");
    coordinator
        .fail("first", "lost_result", true)
        .expect("记录结果缺口");
    let receipt = coordinator.receipt("first").expect("原引用");
    assert_eq!(
        receipt.stage,
        CardInstanceStage::OutcomeUnknown,
        "已提交不能声称零输入失败"
    );
    assert!(
        !receipt.failure.expect("失败依据").retry_safe,
        "已提交未知结果不得安全重发"
    );
}

/// 已绑定 turn 的未知结果只能用原实例、原 run/session/turn 的可信终态核对，不能重发。
/// 数据构造：A 已绑定 turn-a 后发生结果缺口；B 已 Enter 但未绑定，自身也标为未知。
/// 执行过程：先送错误 turn、B 的猜测终态和猜测 start，再送 A 的精确成功终态。
/// 预期结果：仅 A 恢复 Succeeded 并清除旧失败资格；B 保持未知，无待发送项与重复正文。
#[test]
fn exact_terminal_evidence_reconciles_only_previously_bound_unknown_turn() {
    use agent::card_instance::CardInstanceTurn;
    let mut coordinator = CardInstanceCoordinator::default();
    let first = assignment("first", 11, "card", None);
    let target = first.target.clone();
    let turn = CardInstanceTurn {
        run_id: Some("run".into()),
        session_id: "session".into(),
        turn_id: "turn-a".into(),
    };
    coordinator.accept(first).expect("A 接收");
    coordinator.submitted("first").expect("A Enter");
    assert!(
        coordinator.observe_start(&target, "task-first", turn.clone()),
        "A 自身 start 绑定精确 turn"
    );
    coordinator
        .fail("first", "lost_result", false)
        .expect("A 未知");
    coordinator
        .accept(assignment("second", 11, "card", None))
        .expect("B 接收");
    coordinator.submitted("second").expect("B Enter");
    coordinator
        .fail("second", "lost_start", false)
        .expect("B 未知");
    let wrong = CardInstanceTurn {
        turn_id: "turn-b".into(),
        ..turn.clone()
    };
    assert!(
        !coordinator.observe_terminal(&target, &wrong, CardInstanceStage::Succeeded, None),
        "猜测 turn 不可结论 A 或未绑定 B"
    );
    assert!(
        !coordinator.observe_start(&target, "task-second", wrong),
        "未知提交不允许依正文猜测恢复绑定"
    );
    assert!(
        coordinator.observe_terminal(&target, &turn, CardInstanceStage::Succeeded, None),
        "原绑定的可信终态可核对 A"
    );
    let first = coordinator.receipt("first").expect("A 原引用");
    assert_eq!(first.stage, CardInstanceStage::Succeeded, "A 精确恢复成功");
    assert_eq!(first.failure, None, "成功证据应清除结果未知的旧失败资格");
    assert_eq!(first.bound_turn, Some(turn), "核对不可重绑定");
    assert_eq!(
        coordinator.receipt("second").expect("B").stage,
        CardInstanceStage::OutcomeUnknown,
        "B 仍需要自身原证据，不能跟随 A 完成"
    );
    assert!(
        coordinator.pending_assignments().is_empty(),
        "核对不产生重发正文"
    );
}
