#[path = "support/workspace_operations.rs"]
mod support;
use agent::card_instance::{CardInstanceEvent, CardInstanceStage};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use storage::BackgroundWorkspaceOperationJournal;
use support::setup;

/// 后台写入失败必须保留原事件并关闭传输authority，文件恢复后同事件继续保存。
/// 数据构造：接收并Armed A，把AppDB临时移走且在原路径放目录制造真实SQLite错误。
/// 执行过程：register authority，enqueue waiting event，等失败后恢复文件并等writer重试成功。
/// 预期结果：failure时gate=false且pending=1，恢复后waiting_reason持久且pending=0；gate不自动复活。
#[test]
fn journal_keeps_failed_events_and_revokes_input_until_durable_retry() {
    let (temp, store, request) = setup();
    store.accept(&request).expect("应接收");
    store.arm_input("assignment-a").expect("应armed");
    let mut receipt = store
        .assignment("assignment-a")
        .expect("应查询")
        .expect("存在")
        .receipt;
    receipt.stage = CardInstanceStage::WaitingInput;
    receipt.waiting_reason = Some("startup_pending".into());
    let gate = Arc::new(AtomicBool::new(true));
    let journal =
        BackgroundWorkspaceOperationJournal::new(store.clone()).expect("应启动后台journal");
    journal.watch("assignment-a", gate.clone());
    let path = temp.path().join("app.sqlite");
    let backup = temp.path().join("backup.sqlite");
    std::fs::rename(&path, &backup).expect("应临时移走DB");
    std::fs::create_dir(&path).expect("应制造真实打开失败");
    journal
        .enqueue(CardInstanceEvent {
            receipt,
            observed_at_ms: 10,
            evidence: "startup_pending".into(),
        })
        .expect("进入可靠writer队列");
    wait_for(|| journal.status().last_error.is_some());
    assert!(
        !gate.load(Ordering::Acquire),
        "writer失败应立即撤销后续输入"
    );
    assert_eq!(journal.status().pending_events, 1, "失败事件不能丢失");
    std::fs::remove_dir(&path).expect("应移除故障fixture");
    std::fs::rename(&backup, &path).expect("应恢复原DB");
    wait_for(|| journal.status().pending_events == 0 && journal.status().last_error.is_none());
    assert_eq!(
        store
            .assignment("assignment-a")
            .expect("重读DB")
            .expect("存在")
            .receipt
            .waiting_reason
            .as_deref(),
        Some("startup_pending"),
        "原事件必须被重试持久化"
    );
    assert!(
        !gate.load(Ordering::Acquire),
        "写入恢复不能自行重新授权输送旧输入"
    );
}
fn wait_for(mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !check() {
        assert!(Instant::now() < deadline, "后台状态应在有界时间内到达");
        std::thread::sleep(Duration::from_millis(20));
    }
}
