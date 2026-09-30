use serde_json::{json, Value};
use std::{net::SocketAddr, path::Path, time::Duration};
use storage::{MobileTaskBaselineStore, MobileTaskDeliveryStore, MobileTurnKey, MobileTurnStore};

pub fn block_capture(app_root: &Path, fixture: &Path) {
    std::fs::write(
        fixture.join("hook-path"),
        app_root
            .join("Norma Realtime Validation.app/Contents/MacOS/multisoul-agent-hook")
            .to_str()
            .expect("isolated hook"),
    )
    .expect("real bundled Hook path");
    let database = app_root.join("data/app.sqlite");
    MobileTaskDeliveryStore::new(database.clone())
        .list("fixture", None, None)
        .expect("initialize existing delivery schema");
    rusqlite::Connection::open(database)
        .expect("isolated database")
        .execute_batch(
            "CREATE TRIGGER l3_interrupt_capture BEFORE INSERT ON mobile_task_deliveries
            BEGIN SELECT RAISE(ABORT, 'l3_capture_interrupted'); END;",
        )
        .expect("only block test delivery commit, never fabricate outcomes");
}

pub fn complete(
    app_root: &Path,
    fixture: &Path,
    workspace: &str,
    operation: &str,
) -> MobileTurnKey {
    std::fs::write(fixture.join("release-first"), b"").expect("allow actual CLI completion");
    let database = app_root.join("data/app.sqlite");
    let outcomes = MobileTurnStore::for_workspace(database.clone(), workspace.into());
    // 10s = existing2s audit handoff + up to8s local PTY/Hook scheduling margin.
    super::wait_until(Duration::from_secs(10), || {
        !outcomes
            .after(0, 1)
            .expect("durable Provider outcome")
            .is_empty()
            && std::fs::read_to_string(app_root.join("stderr.log"))
                .is_ok_and(|log| log.contains("l3_capture_interrupted"))
    });
    let events = outcomes.after(0, 2).expect("actual completion");
    assert_eq!(
        events.len(),
        1,
        "必须只有一次真实Provider完成，而非测试注入"
    );
    let outcome = &events[0].outcome;
    assert_eq!(
        outcome.state,
        storage::MobileTurnState::Completed,
        "完成来自真实Stop Hook"
    );
    let baseline = MobileTaskBaselineStore::new(database.clone())
        .get(workspace, &outcome.key)
        .expect("actual task baseline")
        .expect("persisted baseline");
    assert_eq!(
        baseline.operation_id, operation,
        "起点必须属于原实时派工操作"
    );
    assert!(
        MobileTaskDeliveryStore::new(database)
            .get(workspace, &outcome.key)
            .expect("before crash delivery")
            .is_none(),
        "成果commit被故障注入阻止，不能已有成果却声称验证了缺失恢复"
    );
    let bytes = std::fs::read_to_string(fixture.join("Realtime Delivery/result.txt"))
        .expect("actual CLI output");
    assert_eq!(bytes, "realtime complete\n", "Hook之外还必须有真实任务产物");
    std::fs::write(
        app_root.join("completion-before-crash.json"),
        serde_json::to_vec_pretty(
            &json!({"outcome":outcome,"baseline":baseline,"file":bytes,"delivery":null}),
        )
        .expect("evidence JSON"),
    )
    .expect("save actual pre-crash evidence");
    outcome.key.clone()
}

pub fn after_crash(app_root: &Path, project: &Path) {
    rusqlite::Connection::open(app_root.join("data/app.sqlite"))
        .expect("same database")
        .execute_batch("DROP TRIGGER l3_interrupt_capture")
        .expect("remove only test fault");
    std::fs::write(project.join("result.txt"), "unrelated edit after crash\n")
        .expect("later bytes must never replace the lost original capture");
}

pub fn verify(
    app_root: &Path,
    address: SocketAddr,
    cert: Vec<u8>,
    machine: &str,
    workspace: &str,
    route: &Value,
    key: &MobileTurnKey,
) -> Value {
    let mut phone = super::wire::connect(address, cert);
    let request = |pending| {
        json!({"type":"task_deliveries","device_id":"phone-l3","token":"a".repeat(64),
        "pending_only":pending,"request":{"workspace_id":workspace,"route_id":route}})
    };
    let response = super::wire::exchange(&mut phone, machine, "recovered-delivery", request(false));
    let page = &response["result"]["deliveries"];
    let entries = page["items"]
        .as_array()
        .expect("real restarted App delivery response");
    assert_eq!(entries.len(), 1, "重启后缺失成果必须恢复为唯一准确记录");
    assert_eq!(
        entries[0]["outcome"]["key"],
        json!(key),
        "保留原node/run/session/turn"
    );
    assert_eq!(
        entries[0]["outcome"]["state"], "completed",
        "真实完成不能降级为中断"
    );
    assert_eq!(
        entries[0]["status"], "window_closed",
        "只能明确不可用，不能重采后续文件"
    );
    assert_eq!(entries[0]["file_count"], 0, "不能把重启后文件当作本轮成果");
    assert!(entries[0]["acceptance"].is_null(), "不能自动替用户验收");
    let inbox = super::wire::exchange(&mut phone, machine, "pending", request(true));
    assert_eq!(
        inbox["result"]["deliveries"]["pending_count"], 0,
        "缺失成果不得成为可验收交付"
    );
    assert_eq!(
        inbox["result"]["deliveries"]["items"],
        json!([]),
        "待验收列表不能冒出伪造文件"
    );
    let saved = MobileTaskDeliveryStore::new(app_root.join("data/app.sqlite"))
        .get(workspace, key)
        .expect("reopened store")
        .expect("durable recovered delivery");
    assert_eq!(
        json!(saved.outcome),
        entries[0]["outcome"],
        "接口必须读取同一持久终态"
    );
    assert!(
        matches!(
            saved.content,
            storage::TaskDeliveryContent::Unavailable { .. }
        ),
        "磁盘不能保存后续字节"
    );
    json!({"response":response,"inbox":inbox,"saved":saved})
}
