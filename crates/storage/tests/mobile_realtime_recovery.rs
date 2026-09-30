use serde_json::json;
use storage::mobile_command_store::{MobileCommandStore, RealtimeCall, RealtimeReservation};

fn call(index: usize) -> RealtimeCall {
    RealtimeCall {
        device_id: "phone-a".into(),
        call_id: "rtc_A".into(),
        turn_id: "utterance-a".into(),
        workspace_id: "project-a".into(),
        operation_id: format!("rt-{index}"),
        request_hash: "a".repeat(64),
    }
}

/// 重开存储不能清空20次预算，重复的已完成调用回读且不再占一次。
///
/// 数据构造：20 = 既有单回合预算；第21次 = 20+1必须拒绝，accepted仅表示任务被接收。
/// 执行过程：每次用新store占位并落盘 → 重放第1次 → 第21次 → 同一operation改参。
/// 预期结果：原回执可回读；预算不重置；改变参数不能复用旧成功。
#[test]
fn durable_turn_budget_survives_reopening_and_replay() {
    if let Ok(path) = std::env::var("NORMA_REALTIME_BUDGET_RESTART") {
        let store = MobileCommandStore::new(path.into());
        assert!(
            matches!(
                store.reserve_realtime(&call(20)).expect("独立进程恢复预算"),
                RealtimeReservation::Blocked(_)
            ),
            "真正重启进程后也不能执行第21次"
        );
        assert!(
            matches!(store.reserve_realtime(&call(0)).expect("独立进程回读"), RealtimeReservation::Existing(value) if value["task_id"] == 41),
            "独立进程必须读取原任务41，不能重新占位"
        );
        return;
    }
    let directory = tempfile::tempdir().expect("隔离数据库");
    let path = directory.path().join("app.sqlite");
    for index in 0..20 {
        let store = MobileCommandStore::new(path.clone());
        assert!(
            matches!(
                store.reserve_realtime(&call(index)).expect("持久占位"),
                RealtimeReservation::New
            ),
            "预算内应只创建一次占位"
        );
        store
            .finish_realtime(&call(index), json!({"status":"accepted","task_id":41}))
            .expect("回执持久化");
    }
    let child = std::process::Command::new(std::env::current_exe().expect("测试程序"))
        .args([
            "--exact",
            "durable_turn_budget_survives_reopening_and_replay",
            "--nocapture",
        ])
        .env("NORMA_REALTIME_BUDGET_RESTART", &path)
        .output()
        .expect("独立恢复进程");
    assert!(
        child.status.success(),
        "独立进程恢复失败：{}",
        String::from_utf8_lossy(&child.stdout)
    );
    let reopened = MobileCommandStore::new(path);
    assert!(
        matches!(reopened.reserve_realtime(&call(0)).expect("回读"),
        RealtimeReservation::Existing(value) if value["task_id"] == 41 && value["status"] == "accepted"),
        "重放只返回任务41的原始接收回执"
    );
    assert!(
        matches!(
            reopened.reserve_realtime(&call(20)).expect("预算判断"),
            RealtimeReservation::Blocked(_)
        ),
        "新进程仍必须阻止第21次调用"
    );
    let mut changed = call(0);
    changed.request_hash = "b".repeat(64);
    assert!(
        reopened.reserve_realtime(&changed).is_err(),
        "改参不能冒充旧操作"
    );
}

/// 已占位未返回的工具会冻结后续调用，跨存储实例依然只允许回读原操作。
///
/// 数据构造：一项inflight操作0、后续操作1；不构造任何确定接收的任务。
/// 执行过程：操作0占位 → 重开store → 重查0和请求1 → 写入真实unknown → 再次请求1。
/// 预期结果：0可核对但不会重派；1在崩溃和unknown两种状态下都不能执行。
#[test]
fn interrupted_reservation_remains_frozen_until_proven() {
    let directory = tempfile::tempdir().expect("隔离数据库");
    let path = directory.path().join("app.sqlite");
    let store = MobileCommandStore::new(path.clone());
    assert!(
        matches!(
            store.reserve_realtime(&call(0)).expect("占位"),
            RealtimeReservation::New
        ),
        "第一个调用必须在执行前占位"
    );
    let reopened = MobileCommandStore::new(path);
    assert!(
        matches!(
            reopened.reserve_realtime(&call(0)).expect("核对"),
            RealtimeReservation::Pending
        ),
        "未完成的原调用只能等待回执"
    );
    assert!(
        matches!(
            reopened.reserve_realtime(&call(1)).expect("冻结"),
            RealtimeReservation::Blocked(_)
        ),
        "未确认操作存在时不能启动下一工具"
    );
    reopened
        .finish_realtime(
            &call(0),
            json!({"status":"unknown","message":"请核对原回执"}),
        )
        .expect("持久未知结果");
    assert!(
        matches!(
            reopened.reserve_realtime(&call(1)).expect("unknown冻结"),
            RealtimeReservation::Blocked(_)
        ),
        "unknown状态不能因重连或落盘而消失"
    );
    assert!(
        !matches!(
            reopened.reserve_realtime(&call(0)).expect("原回执"),
            RealtimeReservation::New
        ),
        "未知操作永远不能重新获得执行占位"
    );
    let page = reopened
        .realtime_operations("phone-a", "project-a", 0, "new-owner")
        .expect("只读核对");
    assert_eq!(page.len(), 1, "分页应包含崩溃前的原操作");
    assert_eq!(page[0].operation_id, "rt-0", "核对不能新建操作1");
    assert!(
        reopened
            .realtime_operations("phone-b", "project-a", 0, "new-owner")
            .expect("另一设备")
            .is_empty(),
        "不能读取其他设备的操作"
    );
}
