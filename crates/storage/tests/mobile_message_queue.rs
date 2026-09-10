use serde_json::json;
use storage::{MobileCommandReservation, MobileCommandStore, NewMobileCommand, NewQueuedMessage};

fn request(device: &str, operation: &str) -> NewMobileCommand {
    NewMobileCommand {
        device_id: device.into(),
        operation_id: operation.into(),
        workspace_id: "project-a".into(),
        route_id: "route-a".into(),
        owner_id: "owner-a".into(),
        request_hash: "a".repeat(64),
    }
}
fn message(text: &str) -> NewQueuedMessage {
    NewQueuedMessage {
        node_id: 7,
        profile: "Claude".into(),
        run_id: "run-a".into(),
        session_id: "session-a".into(),
        grant_revision: 2,
        text: text.into(),
    }
}
fn receipt(turn: &str) -> serde_json::Value {
    json!({"node_id":7,"run_id":"run-a","session_id":"session-a","turn_id":turn})
}

/// 排队内容与已接收回执原子持久化，跨手机同卡 FIFO，实际回执不允许认领其他运行。
///
/// 数据构造：phone-a/first、phone-b/second 向同 node=7 排队，正文保留 LF/Tab；grant revision=2。
/// 执行过程：入队 → 重开查原文与回执 → 重复/改参数 → 尝试后项先发 → 首项 claim → 错回执 → 正确完成 → 第二项 claim。
/// 预期结果：仅两个条目，准确 FIFO；入队不能显示 sent；旧会话回执不能覆盖，首项终态不可修改；跨项目不可读取。
#[test]
fn queue_persists_exact_content_and_serializes_each_card() {
    let dir = tempfile::tempdir().expect("目录");
    let path = dir.path().join("app.sqlite");
    let store = MobileCommandStore::new(path.clone());
    let first = request("phone-a", "first");
    let second = request("phone-b", "second");
    assert!(
        matches!(
            store
                .enqueue_message(&first, &message(" \nfirst\t\n"))
                .expect("首条"),
            MobileCommandReservation::New
        ),
        "首条必须创建一次"
    );
    assert!(
        matches!(
            store
                .enqueue_message(&second, &message("second"))
                .expect("次条"),
            MobileCommandReservation::New
        ),
        "次条独立入队"
    );
    let store = MobileCommandStore::new(path);
    let queued = store.queued_messages("owner-a").expect("恢复");
    assert_eq!(queued.len(), 2, "两个已接收消息都应落盘");
    assert_eq!(queued[0].request.operation_id, "first", "首项保持提交顺序");
    assert_eq!(
        queued[1].request.operation_id, "second",
        "另一手机的次项不可插队"
    );
    assert_eq!(
        queued[0].message.text, " \nfirst\t\n",
        "正文与空白不能截断或标准化"
    );
    assert_eq!(queued[0].state, "queued", "入队不表示 Provider 已收到");
    assert!(queued[0].result.is_none(), "未执行不得出现 Provider 结果");
    let ack = store
        .get("phone-a", "first", "project-a", "owner-a")
        .expect("回执")
        .expect("有记录");
    assert_eq!(ack.state, "succeeded", "入队回执必须和队列同事务保存");
    assert_eq!(
        ack.result.expect("入队结果")["status"],
        "queued",
        "必须标明这是入队成功"
    );
    assert!(
        matches!(
            store
                .enqueue_message(&first, &message(" \nfirst\t\n"))
                .expect("重试"),
            MobileCommandReservation::Existing(_)
        ),
        "重试不能产生另一次执行权"
    );
    assert!(
        matches!(
            store
                .enqueue_message(&first, &message("changed"))
                .expect("改内容"),
            MobileCommandReservation::Conflict
        ),
        "同 ID 改正文即使传错 hash 也不得覆盖"
    );
    assert!(
        !store.claim_queued_message(&second).expect("后项先发"),
        "同卡后项不可越过首项"
    );
    assert!(
        store.claim_queued_message(&first).expect("领取首项"),
        "首项可获得发送权"
    );
    assert!(
        !store.claim_queued_message(&first).expect("重复领取"),
        "发送权只能领取一次"
    );
    assert!(
        !store.claim_queued_message(&second).expect("首项未确认"),
        "Provider 未确认前后项继续等待"
    );
    let mut wrong = receipt("turn-a");
    wrong["session_id"] = json!("session-b");
    assert!(
        store.finish_queued_message(&first, Ok(wrong)).is_err(),
        "另一会话回执必须拒绝"
    );
    assert_eq!(
        store
            .queued_message("phone-a", "first", "project-a", "owner-a")
            .expect("查询")
            .expect("首项")
            .state,
        "dispatching",
        "错回执不能清除执行中状态"
    );
    store
        .finish_queued_message(&first, Ok(receipt("turn-a")))
        .expect("准确回执");
    store
        .finish_queued_message(&first, Ok(receipt("turn-a")))
        .expect("重复写同终态");
    assert!(
        store
            .finish_queued_message(&first, Ok(receipt("turn-b")))
            .is_err(),
        "终态不能变成另一回合"
    );
    assert!(
        store.claim_queued_message(&second).expect("次项"),
        "首项收到确认后可以领取下一项"
    );
    assert!(
        store
            .queued_message("phone-a", "first", "project-b", "owner-a")
            .expect("跨项目")
            .is_none(),
        "另一项目不得读取队列正文"
    );
    assert!(
        store
            .queued_message("phone-b", "first", "project-a", "owner-a")
            .expect("跨手机")
            .is_none(),
        "另一手机不得读取消息"
    );
    let mut rebooted = first.clone();
    rebooted.owner_id = "owner-b".into();
    assert!(
        matches!(
            store
                .enqueue_message(&rebooted, &message(" \nfirst\t\n"))
                .expect("重启重复入队"),
            MobileCommandReservation::Existing(_)
        ),
        "重启只能返回原入队回执，不能产生新执行权"
    );
    let old = store
        .queued_message("phone-b", "second", "project-a", "owner-b")
        .expect("新 owner 查询")
        .expect("保留");
    assert_eq!(
        old.state, "outcome_unknown",
        "重启时执行中的消息不可再次发送"
    );
    assert!(
        store
            .queued_messages("owner-b")
            .expect("新 owner 扫描")
            .is_empty(),
        "新服务不得接管旧运行队列"
    );
}

/// 取消与开始发送原子竞争；锁库、超限或损坏输入不能留下虚假的入队成功。
///
/// 数据构造：一个待发送项同时 claim/cancel，另一个未发送项在 owner 变化后查询；消息最大32KiB，多1字节拒绝。
/// 执行过程：并发竞争 → 检查唯一赢家 → 查询状态 → 关窗身份变化 → 不可写路径/超限入队。
/// 预期结果：取消成功则永不派发，派发先赢则不能伪装撤回；旧 queued 可明确显示取消，不重放；非法消息无账本条目。
#[test]
fn queue_cancel_competes_atomically_and_old_owners_never_replay() {
    let dir = tempfile::tempdir().expect("目录");
    let store = MobileCommandStore::new(dir.path().join("app.sqlite"));
    let item = request("phone-a", "race");
    store
        .enqueue_message(&item, &message("race"))
        .expect("入队");
    let a = store.clone();
    let first = item.clone();
    let b = store.clone();
    let second = item.clone();
    let claim = std::thread::spawn(move || a.claim_queued_message(&first).expect("领取"));
    let cancel = std::thread::spawn(move || b.cancel_queued_message(&second).expect("取消"));
    let claimed = claim.join().expect("领取线程");
    let cancelled = cancel.join().expect("取消线程");
    assert_ne!(
        claimed, cancelled,
        "必须恰好一个操作成功；不能既取消又发送或都失败"
    );
    let record = store
        .queued_message("phone-a", "race", "project-a", "owner-a")
        .expect("查询")
        .expect("记录");
    assert_eq!(
        record.state,
        if claimed { "dispatching" } else { "cancelled" },
        "状态应与唯一赢家一致"
    );
    if claimed {
        assert!(
            !store.cancel_queued_message(&item).expect("再次取消"),
            "已领取不能当作未发送撤回"
        );
        store
            .finish_queued_message(&item, Err("outcome_unknown".into()))
            .expect("不确定结果");
        assert!(
            !store.claim_queued_message(&item).expect("未知重试"),
            "未知结果不能重放"
        );
    } else {
        assert!(
            !store.claim_queued_message(&item).expect("已取消重试"),
            "取消后永远不可派发"
        );
        assert!(
            store.cancel_queued_message(&item).expect("重复取消"),
            "明确已取消可幂等返回"
        );
    }
    let pending = request("phone-a", "pending");
    store
        .enqueue_message(&pending, &message("keep me"))
        .expect("另一条");
    let closed = store
        .queued_message("phone-a", "pending", "project-a", "owner-b")
        .expect("关闭查询")
        .expect("保留原文");
    assert_eq!(
        closed.state, "cancelled",
        "尚未领取的旧 owner 消息可证明没有执行"
    );
    assert_eq!(closed.message.text, "keep me", "关闭不能丢失已交接的正文");
    let invalid = request("phone-a", "too-large");
    // 32 × 1024 = 32768 UTF-8 bytes; one extra ASCII byte exceeds the native message bound.
    assert!(
        store
            .enqueue_message(&invalid, &message(&"a".repeat(32 * 1024 + 1)))
            .is_err(),
        "超限必须整体拒绝"
    );
    assert!(
        store
            .get("phone-a", "too-large", "project-a", "owner-a")
            .expect("查账本")
            .is_none(),
        "失败入队不得留下成功账本"
    );
    std::fs::write(dir.path().join("blocked"), "file").expect("阻塞路径");
    assert!(
        MobileCommandStore::new(dir.path().join("blocked/app.sqlite"))
            .enqueue_message(&invalid, &message("valid"))
            .is_err(),
        "无法保存不能接收队列"
    );
}

/// 数据库第二次写入失败必须回滚入队回执；队列达到上限时整体拒绝，但重试已有操作仍可查询。
///
/// 数据构造：兼容当前命令账本的数据库；触发器让 queue-abort 入队行失败；128个短正文条目达到数量上限；每条最大32KiB，故活跃正文预算最大128×32KiB=4MiB。
/// 执行过程：注入仅针对该操作的 SQL 失败 → 入队 → 查询两张表 → 移除触发器 → 填满 → 新项拒绝 → 取消首项后入队。
/// 预期结果：失败不留下 succeeded 账本或消息；已满时原消息不丢失，129号无残留；取消释放容量，独立卡片可并行领取。
#[test]
fn queue_ack_rolls_back_on_insert_failure_and_capacity_never_evicts_messages() {
    let dir = tempfile::tempdir().expect("目录");
    let path = dir.path().join("app.sqlite");
    let store = MobileCommandStore::new(path.clone());
    // Use the existing API first: the queue migration must preserve an ordinary command receipt.
    let legacy = request("phone-a", "old-create");
    store.reserve(&legacy).expect("既有普通命令");
    store
        .finish(&legacy, Ok(json!({"node_id":99})))
        .expect("既有结果");
    let connection = rusqlite::Connection::open(&path).expect("注入故障");
    connection
        .execute_batch(
            "CREATE TRIGGER reject_test_queue BEFORE INSERT ON mobile_message_queue
        WHEN NEW.operation_id='queue-abort' BEGIN SELECT RAISE(ABORT,'test write failure'); END;",
        )
        .expect("仅影响指定条目");
    let failed = request("phone-a", "queue-abort");
    assert!(
        store
            .enqueue_message(&failed, &message("keep intact"))
            .is_err(),
        "第二次写入失败不得接受消息"
    );
    assert!(
        store
            .get("phone-a", "queue-abort", "project-a", "owner-a")
            .expect("账本")
            .is_none(),
        "事务必须回滚首条成功账本写入"
    );
    assert!(
        store
            .queued_message("phone-a", "queue-abort", "project-a", "owner-a")
            .expect("队列")
            .is_none(),
        "失败不能留下残缺队列"
    );
    assert_eq!(
        store
            .get("phone-a", "old-create", "project-a", "owner-a")
            .expect("旧账本")
            .expect("保留")
            .result,
        Some(json!({"node_id":99})),
        "迁移和失败不得破坏旧命令结果"
    );
    connection
        .execute_batch("DROP TRIGGER reject_test_queue;")
        .expect("移除测试故障");
    for index in 0..128 {
        store
            .enqueue_message(
                &request("phone-a", &format!("queue-{index}")),
                &message(&format!("message-{index}")),
            )
            .expect("128项以内允许入队");
    }
    let overflow = request("phone-a", "queue-128");
    assert!(
        store
            .enqueue_message(&overflow, &message("overflow"))
            .is_err(),
        "第129个活跃消息超过128上限"
    );
    assert!(
        store
            .get("phone-a", "queue-128", "project-a", "owner-a")
            .expect("容量拒绝")
            .is_none(),
        "超限不能留下虚假入队回执"
    );
    assert_eq!(
        store.queued_messages("owner-a").expect("仍在队列").len(),
        128,
        "容量满不能静默淘汰老消息"
    );
    assert_eq!(
        store
            .queued_message("phone-a", "queue-0", "project-a", "owner-a")
            .expect("首项")
            .expect("存在")
            .message
            .text,
        "message-0",
        "最早消息仍完整保留"
    );
    assert!(
        matches!(
            store
                .enqueue_message(&request("phone-a", "queue-0"), &message("message-0"))
                .expect("满时幂等"),
            MobileCommandReservation::Existing(_)
        ),
        "满队列仍可重查原入队"
    );
    assert!(
        store
            .cancel_queued_message(&request("phone-a", "queue-0"))
            .expect("取消首项"),
        "明确取消可释放容量"
    );
    let mut independent = message("other card");
    independent.node_id = 8;
    store
        .enqueue_message(&overflow, &independent)
        .expect("取消后可入队另一卡");
    assert!(
        store.claim_queued_message(&overflow).expect("独立卡领取"),
        "卡片8不应被卡片7的待发送消息阻塞"
    );
    assert!(
        !store
            .claim_queued_message(&request("phone-a", "queue-2"))
            .expect("原卡顺序"),
        "原卡第3项仍不能越过第2项"
    );
    let mut forged = request("phone-a", "queue-1");
    forged.owner_id = "owner-b".into();
    assert!(
        store.claim_queued_message(&forged).is_err(),
        "另一服务 owner 不能领取旧消息"
    );
    assert!(
        store.cancel_queued_message(&forged).is_err(),
        "另一 owner 不能篡改旧消息"
    );
}

/// 明确的输入前拒绝可以回到等待；超时未知不可重排；其他卡片的回执不能认领该消息。
///
/// 数据构造：一个已领取的node=7条目；node=8的错误回执；同卡后项仍待发送。
/// 执行过程：领取 → 输入前忙碌拒绝/defer → 再领取 → 错节点/错run → outcome_unknown → 尝试defer/claim。
/// 预期结果：未写入的等待状态可恢复；错误身份不能结束条目；未知结果只能查询，不能自动重复执行。
#[test]
fn queue_retries_only_confirmed_pre_input_rejections() {
    let dir = tempfile::tempdir().expect("目录");
    let store = MobileCommandStore::new(dir.path().join("app.sqlite"));
    let item = request("phone-a", "defer");
    store
        .enqueue_message(&item, &message("exact text"))
        .expect("入队");
    assert!(
        store.defer_queued_message(&item).is_err(),
        "未领取不能假装输入前拒绝"
    );
    assert!(
        store.claim_queued_message(&item).expect("领取"),
        "获得一次发送权"
    );
    store.defer_queued_message(&item).expect("明确输入前拒绝");
    let deferred = store
        .queued_message("phone-a", "defer", "project-a", "owner-a")
        .expect("查询")
        .expect("条目");
    assert_eq!(deferred.state, "queued", "输入前拒绝应保留等待");
    assert!(deferred.result.is_none(), "等待不能携带完成回执");
    assert!(
        store.claim_queued_message(&item).expect("再次就绪"),
        "确认未发送后才可重新领取"
    );
    let mut wrong = receipt("turn-a");
    wrong["node_id"] = json!(8);
    assert!(
        store.finish_queued_message(&item, Ok(wrong)).is_err(),
        "另一卡片不能认领"
    );
    let mut wrong = receipt("turn-a");
    wrong["run_id"] = json!("run-b");
    assert!(
        store.finish_queued_message(&item, Ok(wrong)).is_err(),
        "另一运行不能认领"
    );
    store
        .finish_queued_message(&item, Err("outcome_unknown".into()))
        .expect("未知结果");
    assert!(
        store.defer_queued_message(&item).is_err(),
        "未知执行结果不得退回等待"
    );
    assert!(
        !store.claim_queued_message(&item).expect("未知领取"),
        "未知消息不得重放"
    );
    assert_eq!(
        store
            .queued_message("phone-a", "defer", "project-a", "owner-a")
            .expect("未知查询")
            .expect("保留")
            .error
            .as_deref(),
        Some("outcome_unknown"),
        "原未知结果应保持可查询"
    );
}
