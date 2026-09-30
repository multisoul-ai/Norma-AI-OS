use super::*;

/// 明确未发送后，新的显式确认只能替代所预览的那次决定，原回执与指令保留。
///
/// 数据构造：node7的首次发送因card_changed在输入前拒绝；A/B两部手机竞争retry-a/retry-b。
/// 执行过程：原决定accepted时重排失败 → 明确拒绝 → 两方带同一旧决定重排 → 旧请求回查。
/// 预期结果：未知/已开始没有重排；只产生一个新执行权，旧回执仍为rejected，原建议不改写。
#[test]
fn todo_explicit_retry_requires_exact_non_delivery_and_keeps_old_receipt() {
    let (_directory, store, first, draft) = fixture();
    let choice = PlanTodoChoice::Accept {
        target: Box::new(draft.todos[0].target.clone()),
        queue: false,
    };
    store
        .reserve_plan_todo(&first, "plan-a", 0, &choice, 2)
        .expect("初次确认");
    let previous = storage::TodoDecisionRef {
        device_id: first.device_id.clone(),
        operation_id: first.operation_id.clone(),
    };
    let mut a = first.clone();
    a.operation_id = "retry-a".into();
    a.request_hash = "b".repeat(64);
    assert!(
        matches!(
            store
                .retry_plan_todo(&a, "plan-a", 0, &choice, 2, &previous)
                .expect("未确认送达"),
            PlanTodoReservation::AlreadyDecided
        ),
        "accepted不能安全重排"
    );
    assert!(
        store
            .get("phone-a", "retry-a", &a.workspace_id, "boot-a")
            .expect("查拒绝")
            .is_none(),
        "被拒绝的重排不得产生命令"
    );
    store
        .finish(&first, Err("card_changed".into()))
        .expect("明确输入前拒绝");
    let before = store
        .plan_todo_decisions(&first.workspace_id, "plan-a", "boot-a")
        .expect("读取旧决定");
    assert!(before[0].can_retry, "明确未发送应允许再次确认");
    let mut b = a.clone();
    b.device_id = "phone-b".into();
    b.operation_id = "retry-b".into();
    let left = (store.clone(), a.clone(), choice.clone(), previous.clone());
    let right = (store.clone(), b.clone(), choice.clone(), previous.clone());
    let one = std::thread::spawn(move || {
        left.0
            .retry_plan_todo(&left.1, "plan-a", 0, &left.2, 2, &left.3)
            .expect("A重排")
    });
    let two = std::thread::spawn(move || {
        right
            .0
            .retry_plan_todo(&right.1, "plan-a", 0, &right.2, 2, &right.3)
            .expect("B重排")
    });
    let results = [one.join().expect("A"), two.join().expect("B")];
    assert_eq!(
        results
            .iter()
            .filter(|x| matches!(x, PlanTodoReservation::New(_)))
            .count(),
        1,
        "只有一个重排执行权"
    );
    assert_eq!(
        results
            .iter()
            .filter(|x| matches!(x, PlanTodoReservation::AlreadyDecided))
            .count(),
        1,
        "另一方旧预览必须失效"
    );
    let current = store
        .plan_todo_decisions(&first.workspace_id, "plan-a", "boot-a")
        .expect("当前决定");
    assert_eq!(current.len(), 1, "每项Todo只显示当前尝试");
    assert_eq!(
        current[0].retry_of.as_ref(),
        Some(&previous),
        "新尝试必须保留原决定身份"
    );
    assert!(!current[0].can_retry, "正在提交的新尝试不能再次重排");
    assert_ne!(
        current[0].receipt.operation_id, first.operation_id,
        "当前结果必须是新尝试"
    );
    let old = store
        .reserve_plan_todo(&first, "plan-a", 0, &choice, 2)
        .expect("重复旧请求");
    match old {
        PlanTodoReservation::Existing(receipt) => {
            assert_eq!(receipt.state, "rejected", "旧回执不可被新尝试改写")
        }
        _ => panic!("旧请求只能回查原回执"),
    }
    let winner = if matches!(results[0], PlanTodoReservation::New(_)) {
        &a
    } else {
        &b
    };
    assert!(
        matches!(
            store
                .retry_plan_todo(winner, "plan-a", 0, &choice, 2, &previous)
                .expect("重查新请求"),
            PlanTodoReservation::Existing(_)
        ),
        "丢失新响应不能再次执行"
    );
    store
        .finish(winner, Err("outcome_unknown".into()))
        .expect("结果未知");
    assert!(
        !store
            .plan_todo_decisions(&first.workspace_id, "plan-a", "boot-a")
            .expect("未知结果")[0]
            .can_retry,
        "未知结果不得重排"
    );
    assert_eq!(
        store
            .get(
                &first.device_id,
                &first.operation_id,
                &first.workspace_id,
                "boot-a"
            )
            .expect("旧记录")
            .expect("保留旧记录")
            .state,
        "rejected",
        "原命令不删除"
    );
}

/// 旧owner尚未领取的队列在重排事务内先取消；已领取或未知队列绝不能重排。
///
/// 数据构造：同计划0/1两项，queue0已领取，queue1仍待发；新服务身份为boot-b。
/// 执行过程：尝试替代已领取项失败 → 终态未知仍不可重排 → 替代旧待发项 → 旧owner再领取。
/// 预期结果：旧待发项永久cancelled，新任务只入队一次；已领取项不变，原文与旧回执保留。
#[test]
fn todo_retry_cancels_old_owner_queue_atomically_but_never_replays_claimed_input() {
    let (_dir, store, mut first, draft) = fixture();
    let choice = PlanTodoChoice::Accept {
        target: Box::new(draft.todos[0].target.clone()),
        queue: true,
    };
    first.operation_id = "claimed".into();
    store
        .reserve_plan_todo(&first, "plan-a", 0, &choice, 2)
        .expect("原队列");
    assert!(
        store.claim_queued_message(&first).expect("领取"),
        "第一项先取得派发权"
    );
    let mut retry = first.clone();
    retry.operation_id = "new-claimed".into();
    retry.owner_id = "boot-b".into();
    let previous = storage::TodoDecisionRef {
        device_id: first.device_id.clone(),
        operation_id: first.operation_id.clone(),
    };
    assert!(
        matches!(
            store
                .retry_plan_todo(&retry, "plan-a", 0, &choice, 2, &previous)
                .expect("领取后重排"),
            PlanTodoReservation::AlreadyDecided
        ),
        "已领取消息不能被另一owner重排"
    );
    store
        .finish_queued_message(&first, Err("outcome_unknown".into()))
        .expect("保留未知");
    assert!(
        !store
            .plan_todo_decisions(&first.workspace_id, "plan-a", "boot-b")
            .expect("未知资格")[0]
            .can_retry,
        "未知派发没有安全重排资格"
    );
    let mut waiting = first.clone();
    waiting.operation_id = "waiting".into();
    store
        .reserve_plan_todo(&waiting, "plan-a", 1, &choice, 2)
        .expect("待发项");
    retry.operation_id = "new-waiting".into();
    let previous = storage::TodoDecisionRef {
        device_id: waiting.device_id.clone(),
        operation_id: waiting.operation_id.clone(),
    };
    assert!(
        store
            .plan_todo_decisions(&first.workspace_id, "plan-a", "boot-b")
            .expect("待发资格")[1]
            .can_retry,
        "新owner可显式确认从未领取的项"
    );
    assert!(
        matches!(
            store
                .retry_plan_todo(&retry, "plan-a", 1, &choice, 2, &previous)
                .expect("安全重排"),
            PlanTodoReservation::New(_)
        ),
        "旧待发取消和新排队必须一起提交"
    );
    assert!(
        !store.claim_queued_message(&waiting).expect("旧owner领取"),
        "重排后原队列不再可领取"
    );
    assert_eq!(
        store
            .queued_message(
                &waiting.device_id,
                &waiting.operation_id,
                &waiting.workspace_id,
                "boot-a"
            )
            .expect("原队列")
            .expect("仍保留")
            .state,
        "cancelled",
        "不能只在手机投影取消状态"
    );
    let current = store
        .plan_todo_decisions(&first.workspace_id, "plan-a", "boot-b")
        .expect("新安排");
    assert_eq!(
        current[1].receipt.operation_id, "new-waiting",
        "当前决定必须关联新任务"
    );
    assert_eq!(
        current[1].queued.as_ref().expect("新队列").message.text,
        "写清登录行为。",
        "重排必须取该Todo原文"
    );
    assert!(!current[1].can_retry, "新排队不能再次重排");
    assert!(
        store
            .get("phone-a", "new-claimed", &first.workspace_id, "boot-b")
            .expect("失败请求")
            .is_none(),
        "不可重排的请求不能留下命令"
    );
}
