use super::test_support::Fixture;
use norma::connector::Clock;
use serde_json::json;
use std::sync::{
    atomic::{AtomicI64, Ordering},
    Arc,
};
struct StepClock(AtomicI64);
impl Clock for StepClock {
    fn now(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}
fn advance_preflight(f: &mut Fixture, clock: &Arc<StepClock>) {
    let clock = clock.clone();
    f.on_next_capture = Some(Box::new(move || {
        clock.0.fetch_add(1, Ordering::SeqCst);
    }));
}

/// 场景：建卡、派工和取消的目标预检各跨过一个 Unix 秒，不能沿用工具入口时间。
/// 构造：临时项目、可控时钟从 1_000_000 开始；每次首次 live capture 加一秒。
/// 执行：三种真实工具调用及同键重放，查询各预留行。
/// 预期：三个时间分别为 1_000_001/2/3，等于预留时间；重放不变，取消目标一致。
#[test]
fn connector_receipt_times_follow_preflight_and_persisted_reservation() {
    let clock = Arc::new(StepClock(AtomicI64::new(1_000_000)));
    let mut f = Fixture::with_clock(clock.clone());
    advance_preflight(&mut f, &clock);
    let created = f.create("timed-create", 0, None);
    assert_eq!(created["state"], "loaded", "建卡成功: {created}");
    assert_eq!(created["created_at"], 1_000_001, "建卡取预检之后的预留时间");
    let dispatch_args = json!({"operation_id":"timed-dispatch","card_id":created["card_id"],"card_revision":created["card_revision"],"task":"临时任务"});
    advance_preflight(&mut f, &clock);
    let dispatched = f.call("dispatch_task", dispatch_args.clone());
    assert_eq!(dispatched["accepted_at"], 1_000_002, "派工取自身预留时间");
    assert!(dispatched["task_id"].is_string(), "派工有目标任务");
    let cancel_args = json!({"operation_id":"timed-cancel","task_id":dispatched["task_id"]});
    advance_preflight(&mut f, &clock);
    let cancelled = f.call("cancel_task", cancel_args.clone());
    assert_eq!(cancelled["state"], "cancelled", "未提交目标取消");
    assert_eq!(cancelled["requested_at"], 1_000_003, "取消取自身预留时间");
    clock.0.store(1_000_010, Ordering::SeqCst);
    for (receipt, replay, field, at) in [
        (
            created.clone(),
            f.create("timed-create", 0, None),
            "created_at",
            1_000_001,
        ),
        (
            dispatched,
            f.call("dispatch_task", dispatch_args),
            "accepted_at",
            1_000_002,
        ),
        (
            cancelled,
            f.call("cancel_task", cancel_args),
            "requested_at",
            1_000_003,
        ),
    ] {
        assert_eq!(replay[field], receipt[field], "{field} 重放不改时间");
        assert_eq!(replay["replayed"], true, "只读原回执");
        let row = storage::MobileCommandStore::new(f.repo.database.clone())
            .connector_receipts(&format!("connector:{}", f.identity.client_epoch()))
            .expect("预留回执")
            .into_iter()
            .find(|r| r.operation_id == receipt["operation_id"].as_str().expect("操作编号"))
            .expect("原操作行");
        assert_eq!(row.created_at_unix, at, "{field} 与原预留行一致");
        assert_ne!(row.created_at_unix, 1_000_000, "不采用 MCP 入口时间");
    }
}
