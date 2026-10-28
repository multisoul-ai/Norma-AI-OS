use serde_json::json;
use storage::question_delivery_store::{
    QuestionDeliveryDraft, QuestionDeliveryReservation, QuestionDeliveryState,
    QuestionDeliveryStore,
};

/// 场景：同一题的持久化提交只能占有一次，进程重启不能把未知结果当成待重发。
/// 构造：临时SQLite文件、完整目标JSON、Green答案；第二请求换delivery ID但目标不变。
/// 执行：reserve首次意图、重开store重复reserve、确认工具消费，再重开读取。
/// 预期：只首次得到lease，重启只返回Unknown；消费证据不可改写，答案不能替换成Blue。
#[test]
fn delivery_intent_is_durable_exclusive_and_never_reclaimed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.sqlite");
    let store = QuestionDeliveryStore::new(path.clone());
    let mut draft = QuestionDeliveryDraft {
        delivery_id: "delivery-1".into(),
        target: json!({"workspace_id":"w","project_id":"p","node_id":7,"provider":"Claude",
            "run_id":"r","connection_id":"c","session_id":"s","request_id":"tool-1","revision":1}),
        question: json!({"items":[{"key":"q","prompt":"Color?","options":["Green","Blue"]}]}),
        answers: json!([{"key":"q","selected":["green"]}]),
    };
    let QuestionDeliveryReservation::Claimed(lease) = store.reserve(&draft).unwrap() else {
        panic!("首次意图应取得唯一持久化资格");
    };
    let restarted = QuestionDeliveryStore::new(path);
    draft.delivery_id = "delivery-2".into();
    let QuestionDeliveryReservation::Existing(receipt) = restarted.reserve(&draft).unwrap() else {
        panic!("重启不得重新占有同题");
    };
    assert_eq!(
        receipt.delivery_id, "delivery-1",
        "重复请求复用首次delivery身份"
    );
    assert_eq!(
        receipt.state,
        QuestionDeliveryState::Unknown,
        "无消费证据不能因重启再次发送"
    );
    store.confirm(&lease, "PostToolUse tool-1 Green").unwrap();
    let receipt = restarted.get(&draft.target).unwrap().unwrap();
    assert_eq!(
        receipt.state,
        QuestionDeliveryState::Answered,
        "持久化消费证据恢复成功状态"
    );
    assert_eq!(
        receipt.evidence.as_deref(),
        Some("PostToolUse tool-1 Green"),
        "消费证据可追溯"
    );
    assert!(
        store.confirm(&lease, "another tool").is_err(),
        "已确认回执不可改写"
    );
    draft.answers = json!([{"key":"q","selected":["blue"]}]);
    assert!(
        restarted.reserve(&draft).is_err(),
        "同一目标不可用不同答案重占"
    );
}

/// 场景：原题已取消、过期或由他方消费时，持久化不能冒认为我方成功，也不能重新占有。
/// 构造：每个终态各一个隔离数据库与原题意图；保留唯一lease。
/// 执行：保存终态、重开读取、同证据重试，再尝试覆盖状态及重复reserve。
/// 预期：精确终态可恢复且幂等；证据/归属不可变，重启不提供新发送资格。
#[test]
fn question_terminal_outcomes_are_durable_immutable_and_not_replayable() {
    for state in [
        QuestionDeliveryState::AnsweredElsewhere,
        QuestionDeliveryState::ConsumedUnattributed,
        QuestionDeliveryState::Cancelled,
        QuestionDeliveryState::Expired,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite");
        let store = QuestionDeliveryStore::new(path.clone());
        let draft = QuestionDeliveryDraft {
            delivery_id: "delivery".into(),
            target: json!({"workspace_id":"w","project_id":"p","node_id":7,"provider":"Cursor","run_id":"r","connection_id":"c","session_id":"s","request_id":"call","revision":1}),
            question: json!({"prompt":"Color?"}),
            answers: json!(["Green"]),
        };
        let QuestionDeliveryReservation::Claimed(lease) = store.reserve(&draft).unwrap() else {
            panic!("新题须有独占lease");
        };
        store.settle(&lease, state, "original-call-closed").unwrap();
        store.settle(&lease, state, "original-call-closed").unwrap();
        let restarted = QuestionDeliveryStore::new(path);
        let receipt = restarted.get(&draft.target).unwrap().unwrap();
        assert_eq!(receipt.state, state, "终态必须保留精确归属");
        assert_eq!(
            receipt.evidence.as_deref(),
            Some("original-call-closed"),
            "原题证据可恢复"
        );
        assert!(
            store.confirm(&lease, "original-call-closed").is_err(),
            "他方/取消终态不可改为我方Answered"
        );
        assert!(
            store.settle(&lease, state, "different-call").is_err(),
            "不能覆盖原题证据"
        );
        assert!(
            matches!(
                restarted.reserve(&draft).unwrap(),
                QuestionDeliveryReservation::Existing(_)
            ),
            "重启不能重新发送"
        );
    }
}

/// 场景：升级前只有evidence字段的已确认意图，升级后仍保留Answered而不重复发送。
/// 构造：按旧DDL手建数据库，写入旧格式Answered意图；新store触发惰性迁移。
/// 执行：4个并发读取同时升级旧行、再reserve相同题目；确认新状态字段已添加。
/// 预期：旧evidence精确保留，读取推导Answered；迁移后不能获得新lease。
#[test]
fn question_outcome_migration_preserves_legacy_confirmed_receipt() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.sqlite");
    let store = QuestionDeliveryStore::new(path.clone());
    let draft = QuestionDeliveryDraft {
        delivery_id: "legacy".into(),
        target: json!({"workspace_id":"w","project_id":"p","node_id":1,"provider":"Claude","run_id":"r","connection_id":"c","session_id":"s","request_id":"call","revision":1}),
        question: json!({"prompt":"Color?"}),
        answers: json!(["Green"]),
    };
    let QuestionDeliveryReservation::Claimed(lease) = store.reserve(&draft).unwrap() else {
        panic!("首次意图");
    };
    store.confirm(&lease, "legacy-consumption").unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("ALTER TABLE question_deliveries DROP COLUMN outcome;")
        .unwrap();
    drop(db);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
    let readers: Vec<_> = (0..4)
        .map(|_| {
            let barrier = barrier.clone();
            let store = QuestionDeliveryStore::new(path.clone());
            let target = draft.target.clone();
            std::thread::spawn(move || {
                barrier.wait();
                store.get(&target)
            })
        })
        .collect();
    for reader in readers {
        let result = reader.join().unwrap();
        assert!(result.is_ok(), "并发读取旧回执不能重复迁移字段：{result:?}");
        assert_eq!(
            result.unwrap().unwrap().state,
            QuestionDeliveryState::Answered,
            "所有读者保留旧消费终态"
        );
    }
    let restarted = QuestionDeliveryStore::new(path);
    let receipt = restarted.get(&draft.target).unwrap().unwrap();
    assert_eq!(
        receipt.state,
        QuestionDeliveryState::Answered,
        "旧消费回执不能降为未知"
    );
    assert_eq!(
        receipt.evidence.as_deref(),
        Some("legacy-consumption"),
        "旧证据原样保存"
    );
    assert!(
        matches!(
            restarted.reserve(&draft).unwrap(),
            QuestionDeliveryReservation::Existing(_)
        ),
        "迁移不重新占有旧题"
    );
}
