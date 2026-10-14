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
