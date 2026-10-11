#[path = "support/workspace_operations.rs"]
mod support;
use agent::card_instance::{CardInstanceEvent, CardInstanceStage};
use agent::workspace_dispatch::WorkspaceCardTarget;
use storage::WorkspaceOperationStore;
use support::setup;

/// 可信terminal先到而摘要稍后到时，原turn的result_snapshot可补摘要，不改已知终态。
/// 数据构造：A已按exactturn成功但result=None；同target/turn的可信snapshot有结果，另一个伪snapshot换turn。
/// 执行过程：先提交wrong-turn结果、再提交exact结果、最后提交改终态的late snapshot。
/// 预期结果：只同stage+originalturn+可信snapshot可补结果；wrongturn零覆盖，成功不回退。
#[test]
fn exact_terminal_result_snapshot_enriches_history_without_changing_outcome() {
    let (_temp, store, request) = setup();
    store.accept(&request).expect("应接收原操作");
    let mut receipt = finish_a(&store);
    receipt.result=Some(serde_json::from_value(serde_json::json!({"availability":"available","text":"已完成登录修复","truncated":false})).expect("应构造可信结果"));
    let mut wrong = receipt.clone();
    wrong.bound_turn.as_mut().expect("原turn存在").turn_id = "wrong-turn".into();
    assert!(
        store
            .record_events(&[CardInstanceEvent {
                receipt: wrong,
                observed_at_ms: 13,
                evidence: "provider_result_snapshot".into()
            }])
            .is_err(),
        "错误turn的结果必须拒绝"
    );
    assert_eq!(
        store
            .assignment("assignment-a")
            .expect("读取失败后")
            .expect("原项")
            .receipt
            .result,
        None,
        "错误结果不得覆盖历史"
    );
    store
        .record_events(&[CardInstanceEvent {
            receipt: receipt.clone(),
            observed_at_ms: 14,
            evidence: "provider_result_snapshot".into(),
        }])
        .expect("原turn可信结果应补全");
    let stored = store
        .assignment("assignment-a")
        .expect("查询补结果")
        .expect("原项");
    assert_eq!(
        stored.receipt.result, receipt.result,
        "成功Hook后的真实结果不能丢失"
    );
    assert_eq!(
        stored.receipt.stage,
        CardInstanceStage::Succeeded,
        "补摘要不能改任务状态"
    );
    let original: WorkspaceCardTarget = stored.assignment.target.clone();
    receipt.stage = CardInstanceStage::Running;
    store
        .record_events(&[CardInstanceEvent {
            receipt,
            observed_at_ms: 15,
            evidence: "late_snapshot".into(),
        }])
        .expect("迟到运行快照应忽略");
    let stored = store
        .assignment("assignment-a")
        .expect("最终查询")
        .expect("原项");
    assert_eq!(
        stored.assignment.target, original,
        "历史目标不能随结果更新刷新"
    );
    assert_eq!(
        stored.receipt.stage,
        CardInstanceStage::Succeeded,
        "旧运行快照不能撤销成功"
    );
}

pub fn finish_a(store: &WorkspaceOperationStore) -> agent::card_instance::CardInstanceReceipt {
    use agent::card_instance::{CardInstanceEvent, CardInstanceStage, CardInstanceTurn};
    store.arm_input("assignment-a").expect("应armed");
    let mut receipt = store
        .assignment("assignment-a")
        .expect("应读取")
        .expect("A存在")
        .receipt;
    receipt.stage = CardInstanceStage::Submitted;
    store
        .record_events(&[CardInstanceEvent {
            receipt: receipt.clone(),
            observed_at_ms: 10,
            evidence: "enter_confirmed".into(),
        }])
        .expect("应保存Enter");
    receipt.stage = CardInstanceStage::Running;
    receipt.bound_turn = Some(CardInstanceTurn {
        run_id: Some("run-a".into()),
        session_id: "session-a".into(),
        turn_id: "turn-a".into(),
    });
    store
        .record_events(&[CardInstanceEvent {
            receipt: receipt.clone(),
            observed_at_ms: 11,
            evidence: "provider_turn_started".into(),
        }])
        .expect("应保存exactstart");
    receipt.stage = CardInstanceStage::Succeeded;
    store
        .record_events(&[CardInstanceEvent {
            receipt: receipt.clone(),
            observed_at_ms: 12,
            evidence: "provider_turn_terminal".into(),
        }])
        .expect("应保存exactterminal");
    receipt
}
