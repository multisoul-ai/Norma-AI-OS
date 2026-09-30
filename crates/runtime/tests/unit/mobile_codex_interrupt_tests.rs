use super::tests::{advance, request, setup, wait_for_audit};
use super::*;

/// 手机中断绑定精确回合，撤销尚未提交的审批；RPC 接收不等于执行已经停止。
///
/// 数据构造：run-a/turn-a 的审批 7 选择 Allow，SQLite 写锁阻止落盘；迟到审批 9，下一回合的审批 11。
/// 执行过程：选择批准待落盘 → 错身份中断被拒绝 → 精确中断 → 收 RPC ack 及迟到审批 → Provider interrupted → 新回合。
/// 预期结果：中断前后没有发出批准；ack 后回执仍等待真实终态，最终为 interrupted；新回合可以重新审批。
#[test]
fn interrupt_revokes_choices_and_waits_for_provider_completion() {
    let (mut runtime, id, directory, server) = setup();
    advance(&runtime, id, vec![request(json!(7))]);
    let pending = runtime
        .mobile_codex_approvals(id)
        .expect("应读审批")
        .remove(0);
    wait_for_audit(&mut runtime, id, &directory, "pending");
    let lock = rusqlite::Connection::open(directory.path().join("audit.sqlite")).expect("测试连接");
    lock.execute_batch("BEGIN IMMEDIATE")
        .expect("应阻止批准意图提交");
    runtime
        .decide_mobile_codex_approval(id, "run-a", &pending.id, ApprovalDecision::Allow)
        .expect("应排队选择");
    assert!(
        runtime
            .interrupt_mobile_codex(id, "old-run", "turn-a")
            .is_err(),
        "旧运行不得中断当前任务"
    );
    assert!(
        runtime
            .interrupt_mobile_codex(id, "run-a", "old-turn")
            .is_err(),
        "旧回合不得中断当前任务"
    );
    let receipt = runtime
        .interrupt_mobile_codex(id, "run-a", "turn-a")
        .expect("应接受精确中断");
    assert!(
        runtime.mobile_codex_interrupt_pending(id, "run-a"),
        "尚未停止时应显示中断请求中"
    );
    assert!(
        runtime
            .interrupt_mobile_codex(id, "run-a", "turn-a")
            .is_err(),
        "同一回合中断不得重复发送"
    );
    assert_eq!(
        advance(&runtime, id, vec![request(json!(9))]),
        json!([]),
        "中断时不应发送任何批准"
    );
    runtime.drain_mobile_codex();
    assert!(
        runtime
            .mobile_codex_approvals(id)
            .expect("应读状态")
            .is_empty(),
        "中断期间迟到审批也必须撤销"
    );
    assert!(receipt.try_recv().is_err(), "RPC ack 不能伪造已经停止");
    assert_ne!(
        runtime.agent_task_state(id),
        Some(AgentTaskState::Ended),
        "Provider 终态之前不能宣称任务已停止"
    );
    lock.execute_batch("COMMIT").expect("应释放写锁");
    wait_for_audit(&mut runtime, id, &directory, "cancelled");
    assert_eq!(
        advance(&runtime, id, vec![]),
        json!([]),
        "迟到落盘回执不得重新发出批准"
    );
    advance(
        &runtime,
        id,
        vec![
            json!({"method":"turn/completed","params":{"threadId":"thread-a","turn":{"id":"turn-a","status":"interrupted"}}}),
        ],
    );
    runtime.drain_mobile_codex();
    assert_eq!(
        receipt
            .try_recv()
            .expect("应收到最终回执")
            .expect("应完成中断"),
        json!({"turnId":"turn-a","status":"interrupted"}),
        "只有真实终态能完成回执"
    );
    assert!(
        !runtime.mobile_codex_interrupt_pending(id, "run-a"),
        "终态后应清除中断请求状态"
    );
    assert_eq!(
        runtime.agent_task_state(id),
        Some(AgentTaskState::Ended),
        "确认中断后任务应结束"
    );
    assert!(
        runtime
            .interrupt_mobile_codex(id, "run-a", "turn-a")
            .is_err(),
        "不能中断已结束的旧回合"
    );
    let mut next = request(json!(11));
    next["params"]["turnId"] = json!("turn-b");
    advance(
        &runtime,
        id,
        vec![
            json!({"method":"turn/started","params":{"threadId":"thread-a","turn":{"id":"turn-b"}}}),
            next,
        ],
    );
    assert_eq!(
        runtime
            .mobile_codex_approvals(id)
            .expect("新回合审批")
            .len(),
        1,
        "前次中断不能阻止用户发起新回合"
    );
    runtime.kill_all();
    assert_eq!(
        server.join().expect("服务端退出"),
        Vec::<Value>::new(),
        "全程不能偷偷发出 Allow"
    );
}

/// 中断失败必须返回错误；任务自然完成时必须返回 completed，不能伪造 interrupted。
///
/// 数据构造：第一个私有 Provider 拒绝中断 RPC；第二个 Provider 在请求后自然完成回合；本地 5s 防挂。
/// 执行过程：第一运行发中断并消费失败 → 核对 Failed/无批准；第二运行发中断 → 发真实 completed → 核对结果。
/// 预期结果：失败运行的控制通道关闭且回执错误；自然完成的运行显示 Done，回执明确为 completed。
#[test]
fn interrupt_failure_and_natural_completion_are_not_reported_as_interrupted() {
    let (mut runtime, id, _directory, server) = setup();
    advance(&runtime, id, vec![json!({"method":"test/rejectInterrupt"})]);
    let receipt = runtime
        .interrupt_mobile_codex(id, "run-a", "turn-a")
        .expect("本地应接受请求");
    let deadline = Instant::now() + Duration::from_secs(5);
    while runtime.mobile_runs.contains_key(&id) && Instant::now() < deadline {
        runtime.drain_mobile_codex();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        receipt.try_recv().expect("应有错误回执").is_err(),
        "Provider 拒绝不能回成功"
    );
    assert!(
        !runtime.mobile_runs.contains_key(&id),
        "中断失败后不能复活已撤销审批"
    );
    assert!(
        matches!(
            runtime.agent_task_state(id),
            Some(AgentTaskState::Failed { .. })
        ),
        "控制失败必须显式可见"
    );
    assert_eq!(
        server.join().expect("失败服务端退出"),
        Vec::<Value>::new(),
        "失败期间不能发送批准"
    );

    let (mut runtime, id, _directory, server) = setup();
    let receipt = runtime
        .interrupt_mobile_codex(id, "run-a", "turn-a")
        .expect("应接受中断请求");
    advance(
        &runtime,
        id,
        vec![
            json!({"method":"turn/completed","params":{"threadId":"thread-a","turn":{"id":"turn-a","status":"completed"}}}),
        ],
    );
    runtime.drain_mobile_codex();
    assert_eq!(
        receipt
            .try_recv()
            .expect("应有真实回执")
            .expect("应报告终态"),
        json!({"turnId":"turn-a","status":"completed"}),
        "自然完成不能冒称被手机中断"
    );
    assert_eq!(
        runtime.agent_task_state(id),
        Some(AgentTaskState::Done),
        "实际 completed 应保留 Done"
    );
    assert!(
        !runtime.mobile_codex_interrupt_pending(id, "run-a"),
        "已结束的请求不应继续等待"
    );
    runtime.kill_all();
    server.join().expect("正常服务端退出");
}
