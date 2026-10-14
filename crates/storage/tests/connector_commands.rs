use serde_json::json;
use storage::{MobileCommandReservation, MobileCommandStore, NewMobileCommand};
fn request() -> NewMobileCommand {
    NewMobileCommand {
        device_id: "connector:epoch-a".into(),
        operation_id: "operation".into(),
        workspace_id: "workspace".into(),
        route_id: "c1.signed.card".into(),
        owner_id: "grokbot".into(),
        request_hash: "a".repeat(64),
    }
}
/// 场景：同一连接器请求重放、换参数、预留后重开；只用临时SQLite。
/// 执行：reserve/finish/lookup，再保留另一个未完成操作；预期：回执不可变、手机不可见、未知不重领。
#[test]
fn connector_ledger_reuses_mobile_receipts_and_isolates_mobile_readers() {
    let dir = tempfile::tempdir().expect("临时库");
    let store = MobileCommandStore::new(dir.path().join("app.sqlite"));
    let request = request();
    assert!(
        matches!(
            store.reserve_connector(&request, 100).expect("预留"),
            MobileCommandReservation::New
        ),
        "首次执行权"
    );
    store
        .finish_connector(&request, Ok(json!({"state":"queued","accepted_at":100})))
        .expect("完成");
    let Some(MobileCommandReservation::Existing(receipt)) =
        store.lookup_connector(&request).expect("重放")
    else {
        panic!("必须返回回执");
    };
    assert_eq!(
        receipt.result.expect("原结果")["accepted_at"],
        100,
        "时间不可变"
    );
    assert!(
        store
            .get(
                &request.device_id,
                &request.operation_id,
                &request.workspace_id,
                "grokbot"
            )
            .expect("手机查询")
            .is_none(),
        "手机回执排除连接器"
    );
    let mut changed = request.clone();
    changed.request_hash = "b".repeat(64);
    assert!(
        matches!(
            store.lookup_connector(&changed).expect("冲突"),
            Some(MobileCommandReservation::Conflict)
        ),
        "异参数不能执行"
    );
    changed = request.clone();
    changed.operation_id = "unfinished".into();
    store.reserve_connector(&changed, 101).expect("未完成预留");
    let Some(MobileCommandReservation::Existing(receipt)) =
        store.lookup_connector(&changed).expect("恢复")
    else {
        panic!("必须读已有记录");
    };
    assert_eq!(
        receipt.state, "outcome_unknown",
        "未完成只核对，永不重新取得执行权"
    );
    assert_eq!(
        store.cleanup_connector(100 + 30 * 86400).expect("边界清理"),
        0,
        "30天整仍保留"
    );
    assert_eq!(
        store.cleanup_connector(102 + 30 * 86400).expect("过期清理"),
        2,
        "超过30天清理"
    );
}

/// 场景：两个并发连接器提交共用mobile_commands；拒绝和未知回执跨重开稳定。
/// 构造：临时SQLite、同键同哈希两个线程、一个手机哨兵；执行reserve、拒绝、重开、清理。
/// 预期：唯一New，原拒绝不可改写，清理仅连接器，绝无connector_operations表。
#[test]
fn connector_concurrency_rejection_and_cleanup_preserve_mobile_commands() {
    let dir = tempfile::tempdir().expect("临时命令库");
    let path = dir.path().join("app.sqlite");
    let store = MobileCommandStore::new(path.clone());
    let source = request();
    let first = store.clone();
    let a = source.clone();
    let second = store.clone();
    let b = source.clone();
    let left = std::thread::spawn(move || first.reserve_connector(&a, 100).expect("首提交"));
    let right = std::thread::spawn(move || second.reserve_connector(&b, 100).expect("重提交"));
    let outcomes = [left.join().expect("首线程"), right.join().expect("次线程")];
    assert_eq!(
        outcomes
            .iter()
            .filter(|r| matches!(r, MobileCommandReservation::New))
            .count(),
        1,
        "只有一个执行权"
    );
    store
        .finish_connector(&source, Err("approval_pending".into()))
        .expect("拒绝持久化");
    let reopened = MobileCommandStore::new(path.clone());
    let Some(MobileCommandReservation::Existing(receipt)) =
        reopened.lookup_connector(&source).expect("重开")
    else {
        panic!("已有回执");
    };
    assert_eq!(receipt.state, "rejected", "执行期拒绝保持终态");
    assert_eq!(
        receipt.error.as_deref(),
        Some("approval_pending"),
        "拒绝原码"
    );
    assert!(
        store
            .finish_connector(&source, Ok(json!({"state":"queued"})))
            .is_err(),
        "条件解除不能覆盖原拒绝"
    );
    let mobile = NewMobileCommand {
        device_id: "phone-a".into(),
        operation_id: "mobile-op".into(),
        workspace_id: "workspace".into(),
        route_id: "route".into(),
        owner_id: "phone-owner".into(),
        request_hash: "b".repeat(64),
    };
    store.reserve(&mobile).expect("手机原入口");
    assert_eq!(
        store
            .cleanup_connector(101 + 30 * 86400)
            .expect("只清连接器"),
        1,
        "连接器过期一行"
    );
    assert!(
        store
            .get("phone-a", "mobile-op", "workspace", "phone-owner")
            .expect("手机原回执")
            .is_some(),
        "手机不被清理"
    );
    assert!(
        store
            .realtime_operations("connector:epoch-a", "workspace", 0, "grokbot")
            .expect("排除连接器")
            .is_empty(),
        "手机实时查询零连接器行"
    );
    let conn = rusqlite::Connection::open(path).expect("临时库schema");
    let forbidden: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='connector_operations'",
            [],
            |r| r.get(0),
        )
        .expect("schema检查");
    assert_eq!(forbidden, 0, "没有第二套操作账本");
}

/// 场景：共享账本内部保存预留时间，但手机回执必须维持原 JSON 契约。
/// 构造：临时库中的普通手机请求；执行预留、完成、查询并序列化。
/// 预期：仅七个原有字段，内部 created_at_unix 仍可供连接器读取。
#[test]
fn mobile_receipt_json_preserves_exact_keys() {
    let dir = tempfile::tempdir().expect("临时库");
    let store = MobileCommandStore::new(dir.path().join("app.sqlite"));
    let mobile = NewMobileCommand {
        device_id: "phone-a".into(),
        operation_id: "mobile-op".into(),
        workspace_id: "workspace".into(),
        route_id: "route".into(),
        owner_id: "phone-owner".into(),
        request_hash: "b".repeat(64),
    };
    store.reserve(&mobile).expect("手机预留");
    store
        .finish(&mobile, Ok(json!({"sent":true})))
        .expect("手机完成");
    let receipt = store
        .get("phone-a", "mobile-op", "workspace", "phone-owner")
        .expect("手机查询")
        .expect("原回执");
    assert!(receipt.created_at_unix > 0, "内部预留时间保留");
    let value = serde_json::to_value(receipt).expect("手机JSON");
    let keys: Vec<_> = value
        .as_object()
        .expect("JSON对象")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "device_id",
            "error",
            "operation_id",
            "result",
            "route_id",
            "state",
            "workspace_id"
        ],
        "手机回执恰好保留七个字段"
    );
    assert!(
        value.get("created_at_unix").is_none(),
        "内部时间不可泄漏进手机协议"
    );
    assert_eq!(value["state"], "succeeded", "手机执行结果保留");
}
