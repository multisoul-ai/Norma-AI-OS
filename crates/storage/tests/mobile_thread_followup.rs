use serde_json::json;
use storage::{
    MobileCommandReservation, MobileCommandStore, NewMobileCommand, NewQueuedMessage,
    NewThreadMessage, ThreadCommandBinding, ThreadMessageStore,
};

/// 原任务追问绑定和队列必须同事务保存，重试与重启不能改派或重发。
///
/// 数据构造：phone在workspace/thread发出一条来源message；追问reply绑定节点7/run-a/session-a。
///   route-a/boot-a为首次目标；route-b/boot-b仅模拟重连的新请求，预期队列数量始终为1。
/// 执行过程：入队 → 同操作换路由重试 → 更换线程/正文 → 用新owner重开查询。
/// 预期结果：原绑定及正文保留；冲突无新队列；重启后未发旧队列明确取消，不自动换路由执行。
#[test]
fn thread_followup_queue_is_atomic_bound_and_never_retargeted() {
    let directory = tempfile::tempdir().expect("隔离目录");
    let path = directory.path().join("app.sqlite");
    let (request, binding, message) = fixture(&path);
    let store = MobileCommandStore::new(path.clone());
    assert!(
        matches!(
            store
                .enqueue_thread_message(&request, &binding, &message)
                .expect("入队"),
            MobileCommandReservation::New
        ),
        "首个追问应持久接收一次"
    );
    let mut retry = request.clone();
    retry.route_id = "route-b".into();
    retry.owner_id = "boot-b".into();
    let receipt = match store
        .enqueue_thread_message(&retry, &binding, &message)
        .expect("重连查询")
    {
        MobileCommandReservation::Existing(receipt) => receipt,
        other => panic!("重复追问应返回原回执，而非{other:?}"),
    };
    assert_eq!(receipt.route_id, "route-a", "重连不能改写原队列的路由");
    let mut wrong = binding.clone();
    wrong.thread_id = "other-thread".into();
    assert!(
        matches!(
            store
                .enqueue_thread_message(&request, &wrong, &message)
                .expect("线程冲突"),
            MobileCommandReservation::Conflict
        ),
        "同操作不得绑定另一个线程"
    );
    let mut changed = message.clone();
    changed.text = "另一条指令".into();
    assert!(
        matches!(
            store
                .enqueue_thread_message(&request, &binding, &changed)
                .expect("正文冲突"),
            MobileCommandReservation::Conflict
        ),
        "同操作不得覆盖队列正文"
    );
    assert_eq!(
        store.queued_messages("boot-a").expect("原队列").len(),
        1,
        "重复与冲突不能添加第二条队列"
    );
    assert!(
        store
            .thread_command(
                "phone",
                "mac",
                "workspace",
                "other-thread",
                "reply",
                "boot-a"
            )
            .expect("错线程读取")
            .is_none(),
        "错线程不可查询此追问"
    );
    let restored = MobileCommandStore::new(path);
    let saved = restored
        .thread_command("phone", "mac", "workspace", "thread", "reply", "boot-b")
        .expect("重启读取")
        .expect("原绑定");
    assert_eq!(saved.binding, binding, "原任务来源与线程归属须跨重启保留");
    let queued = restored
        .queued_message("phone", "reply", "project", "boot-b")
        .expect("重启队列")
        .expect("追问");
    assert_eq!(
        queued.state, "cancelled",
        "旧进程未发队列不可在新路由下自动执行"
    );
    assert_eq!(
        queued.message.text, "为什么这样改？",
        "取消不能丢弃用户的追问"
    );
    assert!(queued.result.is_none(), "未输入不能生成Provider回执");
    assert!(
        restored
            .queued_messages("boot-b")
            .expect("新进程队列")
            .is_empty(),
        "重启与查询不得重建可执行队列"
    );
}

/// 追问保存失败或旧命令碰撞不能留下未绑定的可执行输入。
///
/// 数据构造：1条已送达来源；SQLite触发器仅在绑定INSERT处注入失败；旧普通队列使用legacy操作。
/// 执行过程：缺来源入队 → 绑定写失败 → 查公开队列/命令 → 恢复写入 → 尝试把旧队列冒充线程追问。
/// 预期结果：失败后命令和队列均不存在；恢复只入队一次；旧队列不能被补绑定。
#[test]
fn failed_followup_binding_rolls_back_queue_and_cannot_hijack_legacy_commands() {
    let directory = tempfile::tempdir().expect("隔离目录");
    let path = directory.path().join("app.sqlite");
    let (request, binding, message) = fixture(&path);
    let store = MobileCommandStore::new(path.clone());
    let mut missing = binding.clone();
    missing.source_message_id = "not-delivered".into();
    assert!(
        store
            .enqueue_thread_message(&request, &missing, &message)
            .is_err(),
        "没有已送达来源不能排队执行"
    );
    let database = rusqlite::Connection::open(&path).expect("失败注入连接");
    database
        .execute_batch(
            "CREATE TRIGGER fail_binding BEFORE INSERT ON mobile_thread_commands
        BEGIN SELECT RAISE(ABORT,'test failure'); END;",
        )
        .expect("注入绑定写失败");
    assert!(
        store
            .enqueue_thread_message(&request, &binding, &message)
            .is_err(),
        "绑定失败必须向调用方报告"
    );
    assert!(
        store
            .get("phone", "reply", "project", "boot-a")
            .expect("操作")
            .is_none(),
        "绑定失败后命令必须回滚"
    );
    assert!(
        store.queued_messages("boot-a").expect("队列").is_empty(),
        "绑定失败后不能留下可执行追问"
    );
    database
        .execute_batch("DROP TRIGGER fail_binding;")
        .expect("恢复写入");
    assert!(
        matches!(
            store
                .enqueue_thread_message(&request, &binding, &message)
                .expect("恢复"),
            MobileCommandReservation::New
        ),
        "事务失败后同操作可首次成功入队"
    );
    let mut legacy = request;
    legacy.operation_id = "legacy".into();
    store
        .enqueue_message(&legacy, &message)
        .expect("普通旧队列");
    assert!(
        matches!(
            store
                .enqueue_thread_message(&legacy, &binding, &message)
                .expect("碰撞"),
            MobileCommandReservation::Conflict
        ),
        "旧队列不能冒领原任务追问的绑定"
    );
    assert!(
        store
            .thread_command("phone", "mac", "workspace", "thread", "legacy", "boot-a")
            .expect("旧队列绑定")
            .is_none(),
        "冲突不能创建线程来源"
    );
}

/// 连续追问只沿用同一原任务此前已确认的队列，不借用其他任务或未来操作。
/// 数据构造：first/second两条同来源绑定的存储fixture，later-parent属于另一原任务；同节点7/run-a。
/// 执行过程：首条入队→确认turn-first→第二条入队→插入另一父任务→确认second→查询第二条的前序。
/// 预期结果：未确认不产生前序；second始终只读first；其他父任务和second自身不可成为其前序。
#[test]
fn followup_predecessor_is_confirmed_scoped_and_precedes_current_queue() {
    let directory = tempfile::tempdir().expect("隔离目录");
    let path = directory.path().join("app.sqlite");
    let (first, binding, message) = fixture(&path);
    let store = MobileCommandStore::new(path);
    store
        .enqueue_thread_message(&first, &binding, &message)
        .expect("首条入队");
    let mut second = first.clone();
    second.operation_id = "second".into();
    assert!(
        store
            .latest_thread_message(&second, &binding, "original-task", "runtime-a:3")
            .expect("尚无确认")
            .is_none(),
        "Queued不能充当已确认的Provider回合"
    );
    assert!(
        store.claim_queued_message(&first).expect("领取首条"),
        "首条可以进入原队列"
    );
    store.finish_queued_message(&first, Ok(json!({"node_id":7,"run_id":"run-a","session_id":"session-a","turn_id":"turn-first"})))
        .expect("确认首条");
    store
        .enqueue_thread_message(&second, &binding, &message)
        .expect("第二条入队");
    let restored = store
        .queued_thread_binding(&second)
        .expect("出队绑定")
        .expect("追问必须有绑定");
    assert_eq!(restored, binding, "执行前可恢复准确来源，不能只保留节点");
    let mut wrong = second.clone();
    wrong.route_id = "route-other".into();
    assert!(
        store.queued_thread_binding(&wrong).is_err(),
        "错误route不能得到原绑定"
    );
    let mut unrelated = first.clone();
    unrelated.operation_id = "later-parent".into();
    let mut unrelated_binding = binding.clone();
    unrelated_binding.payload =
        json!({"parent_operation_id":"other-task","task_ref":"other-runtime:3"}).to_string();
    store
        .enqueue_thread_message(&unrelated, &unrelated_binding, &message)
        .expect("另一任务的追问");
    assert!(
        store.claim_queued_message(&second).expect("领取第二条"),
        "第二条先于其他父任务"
    );
    store.finish_queued_message(&second, Ok(json!({"node_id":7,"run_id":"run-a","session_id":"session-a","turn_id":"turn-second"})))
        .expect("确认第二条");
    assert!(
        store
            .claim_queued_message(&unrelated)
            .expect("领取另一任务"),
        "按队列顺序领取"
    );
    store.finish_queued_message(&unrelated, Ok(json!({"node_id":7,"run_id":"run-a","session_id":"session-a","turn_id":"turn-other"})))
        .expect("确认其他任务");
    let previous = store
        .latest_thread_message(&second, &binding, "original-task", "runtime-a:3")
        .expect("查询前序")
        .expect("应找到first");
    assert_eq!(
        previous.request.operation_id, first.operation_id,
        "second的前序只能是first"
    );
    assert_eq!(
        previous.result.as_ref().expect("确认结果")["turn_id"],
        "turn-first",
        "锚点只能来自首条Provider确认"
    );
    assert_ne!(
        previous.request.operation_id, second.operation_id,
        "当前操作不能作为自己的前序"
    );
    assert_ne!(
        previous.request.operation_id, unrelated.operation_id,
        "其他父任务不得污染原追问链"
    );
}

fn fixture(path: &std::path::Path) -> (NewMobileCommand, ThreadCommandBinding, NewQueuedMessage) {
    ThreadMessageStore::new(path.into())
        .receive(&NewThreadMessage {
            device_id: "phone".into(),
            source_mac_id: "mac".into(),
            collaboration_workspace_id: "workspace".into(),
            thread_id: "thread".into(),
            operation_id: "message-op".into(),
            message_id: "message".into(),
            origin_device_id: "origin".into(),
            fingerprint: "a".repeat(64),
            payload: "{}".into(),
        })
        .expect("来源先持久送达");
    (
        NewMobileCommand {
            device_id: "phone".into(),
            operation_id: "reply".into(),
            workspace_id: "project".into(),
            route_id: "route-a".into(),
            owner_id: "boot-a".into(),
            request_hash: "b".repeat(64),
        },
        ThreadCommandBinding {
            source_mac_id: "mac".into(),
            collaboration_workspace_id: "workspace".into(),
            thread_id: "thread".into(),
            source_operation_id: "message-op".into(),
            source_message_id: "message".into(),
            payload: json!({"parent_operation_id":"original-task","task_ref":"runtime-a:3"})
                .to_string(),
        },
        NewQueuedMessage {
            node_id: 7,
            profile: "Codex".into(),
            run_id: "run-a".into(),
            session_id: "session-a".into(),
            grant_revision: 1,
            text: "为什么这样改？".into(),
        },
    )
}
