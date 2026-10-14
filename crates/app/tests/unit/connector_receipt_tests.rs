use super::*;
use agent::AdapterResult;
/// 场景：内部CreateAgent成功回执缺node_id，或执行结果未知，不能伪造建卡成功。
/// 构造：临时库、真实WriteRequest、无节点/unknown AdapterResult；执行回执映射。
/// 预期：card_create_failed/outcome_unknown，且不进入快照读取或第二次执行。
#[test]
fn connector_creation_receipt_requires_node_and_known_outcome() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let (live, _) = async_channel::bounded(1);
    let repo = Repository {
        database: dir.path().join("app.sqlite"),
        clock: std::sync::Arc::new(norma::connector::SystemClock),
        signer: norma::connector::CardSigner::new(&[8; 32]),
        live,
    };
    let request = WriteRequest::parse(
        "create_card",
        json!({"operation_id":"op","workspace_id":"w","area_id":"a","profile":"codex"}),
        &repo.signer,
    )
    .expect("参数");
    let missing = Executed {
        receipt: AdapterResult::succeeded("意外无节点"),
        task_ref: None,
        task_state: None,
        tracking: None,
    };
    assert_eq!(
        receipt(&repo, &request, missing, 100),
        Err("card_create_failed"),
        "缺node_id不能回loaded"
    );
    let unknown = Executed {
        receipt: AdapterResult::unknown("写入中断").with_node_id(7),
        task_ref: None,
        task_state: None,
        tracking: None,
    };
    assert_eq!(
        receipt(&repo, &request, unknown, 100),
        Err("outcome_unknown"),
        "未知执行不得重建"
    );
    assert_eq!(
        super::super::execution::error_code(&agent::AdapterError::Failed("意外回执".into()), true),
        "card_create_failed",
        "CreateAgent异常统一稳定码"
    );
}

/// 场景：协调器把成功终态命名为 succeeded，连接器规格对外使用 done。
/// 构造：临时回执映射器、真实取消请求、已成功任务的 Adapter 回执。
/// 执行：映射取消回执；预期：already_terminal + done，不泄漏内部 succeeded。
#[test]
fn connector_cancel_terminal_state_uses_spec_done() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let (live, _) = async_channel::bounded(1);
    let repo = Repository {
        database: dir.path().join("app.sqlite"),
        clock: std::sync::Arc::new(norma::connector::SystemClock),
        signer: norma::connector::CardSigner::new(&[8; 32]),
        live,
    };
    let request = WriteRequest::parse(
        "cancel_task",
        json!({"operation_id":"cancel-done","task_id":"run:1"}),
        &repo.signer,
    )
    .expect("取消请求");
    let executed = Executed {
        receipt: AdapterResult::succeeded("已结束")
            .with_task_id(1)
            .with_reply_status("succeeded"),
        task_ref: Some("run:1".into()),
        task_state: Some("succeeded".into()),
        tracking: Some("tracked".into()),
    };
    let value = receipt(&repo, &request, executed, 100).expect("取消回执");
    assert_eq!(value["state"], "already_terminal", "终态不能再请求中断");
    assert_eq!(value["terminal_state"], "done", "与进度和现有规格一致");
    assert_ne!(
        value["terminal_state"], "succeeded",
        "内部状态不能漏到连接器"
    );
}

/// 场景：升级前账本已有 terminal_state=succeeded 的成功取消回执，重放也必须使用规格名称 done。
/// 构造：旧终态回执及相同取消请求，requested_at 固定为 100；调用实际账本重放映射。
/// 预期：仅规范旧终态名称和 replayed，任务引用、操作标识、终态结果与预留时间保持不变。
#[test]
fn connector_cancel_replay_normalizes_legacy_terminal_state() {
    let signer = norma::connector::CardSigner::new(&[8; 32]);
    let request = WriteRequest::parse(
        "cancel_task",
        json!({"operation_id":"old-cancel","task_id":"run:1"}),
        &signer,
    )
    .expect("取消请求");
    let original = json!({"operation_id":"old-cancel","task_id":"run:1","state":"already_terminal","terminal_state":"succeeded","requested_at":100,"replayed":false});
    let stored = MobileCommandReceipt {
        device_id: "connector:epoch".into(),
        operation_id: "old-cancel".into(),
        workspace_id: "workspace".into(),
        route_id: "run:1".into(),
        state: "succeeded".into(),
        result: Some(original.clone()),
        error: None,
        created_at_unix: 100,
    };
    let replayed = replay(
        MobileCommandReservation::Existing(Box::new(stored)),
        &request,
    )
    .expect("旧回执重放");
    let mut expected = original;
    expected["terminal_state"] = json!("done");
    expected["replayed"] = json!(true);
    assert_eq!(
        replayed, expected,
        "旧成功任务名称规范为 done，其他回执字段冻结"
    );
    assert_ne!(
        replayed["terminal_state"], "succeeded",
        "重放不能再次漏出内部状态名"
    );
}
