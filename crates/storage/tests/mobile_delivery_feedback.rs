use serde_json::json;
use storage::{
    ApprovalAuditStore, BackgroundApprovalAudit, DeliveryFeedback, MobileCommandStore,
    MobileTaskDeliveryStore, MobileTurnKey, MobileTurnOutcome, MobileTurnState, MobileTurnStore,
    NewMobileCommand, TaskDeliveryContent,
};

/// 修改意见先保存原文与旧成果，再把真实新回合回执绑定同一Provider会话。
///
/// 数据构造：节点7/run/session/old的已完成零文件成果；意见含首尾空格与换行；新turn为new。
/// 执行过程：错指纹拒绝 → 预留反馈 → 原文持久化 → 修改原文拒绝 → 错session/旧turn回执拒绝 → 新turn成功 → 重查。
/// 预期结果：意见和旧指纹不可覆盖；仅同会话新turn成功；旧成果不被验收或改写，另一workspace不可读原文。
#[test]
fn feedback_preserves_origin_and_only_binds_a_new_turn_in_the_original_session() {
    let dir = tempfile::tempdir().expect("隔离数据库");
    let path = dir.path().join("app.sqlite");
    let key = MobileTurnKey {
        node_id: 7,
        profile: "Codex".into(),
        run_id: "run".into(),
        session_id: "session".into(),
        turn_id: "old".into(),
    };
    let writer = BackgroundApprovalAudit::new(
        ApprovalAuditStore::new(dir.path().join("audit.sqlite")).with_turn_store(
            MobileTurnStore::for_workspace(path.clone(), "project".into()),
        ),
    )
    .expect("真实终态存储");
    //5s沿用本地writer提交预算，等待commit后才能冻结交付。
    writer
        .record_turn(MobileTurnOutcome {
            key: key.clone(),
            state: MobileTurnState::Completed,
        })
        .expect("入队")
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("回执")
        .expect("提交");
    let deliveries = MobileTaskDeliveryStore::new(path.clone());
    let original = deliveries
        .record(
            "project",
            &key,
            &TaskDeliveryContent::Available {
                review: workspace::review::ProjectReview {
                    base_commit: None,
                    files: vec![],
                    conflicted: false,
                },
            },
        )
        .expect("冻结成果");
    let source = NewMobileCommand {
        device_id: "phone".into(),
        operation_id: "feedback".into(),
        workspace_id: "project".into(),
        route_id: "route".into(),
        owner_id: "owner".into(),
        request_hash: "a".repeat(64),
    };
    let store = MobileCommandStore::new(path);
    store.reserve(&source).expect("预留原操作");
    let feedback = DeliveryFeedback {
        key: key.clone(),
        fingerprint: original.fingerprint.clone(),
        text: "  请改成黑色\n保留输入框  ".into(),
    };
    let mut wrong = feedback.clone();
    wrong.fingerprint = "b".repeat(64);
    assert!(
        store.prepare_delivery_feedback(&source, &wrong).is_err(),
        "错指纹不能保存为可派发意见"
    );
    assert!(
        store
            .delivery_feedback("phone", "feedback", "project")
            .expect("查原文")
            .is_none(),
        "拒绝后没有反馈占位"
    );
    store
        .prepare_delivery_feedback(&source, &feedback)
        .expect("保存原文");
    store
        .prepare_delivery_feedback(&source, &feedback)
        .expect("原请求幂等");
    wrong = feedback.clone();
    wrong.text.push('!');
    assert!(
        store.prepare_delivery_feedback(&source, &wrong).is_err(),
        "同操作不能替换意见"
    );
    assert_eq!(
        store
            .delivery_feedback("phone", "feedback", "project")
            .expect("重读"),
        Some(feedback.clone()),
        "必须保存原始空格/换行/准确成果"
    );
    assert!(
        store
            .delivery_feedback("phone", "feedback", "foreign")
            .expect("隔离查询")
            .is_none(),
        "其他项目不能读取意见"
    );
    assert!(
        feedback.message().ends_with(&feedback.text),
        "实际发送保留原文，不trim"
    );
    assert!(
        feedback.message().contains(&feedback.fingerprint),
        "Agent消息必须明确旧成果指纹"
    );
    let wrong_session = json!({"node_id":7,"run_id":"run","session_id":"other","turn_id":"new"});
    assert!(
        store.finish(&source, Ok(wrong_session)).is_err(),
        "新会话回执不能借原反馈成功"
    );
    let old_turn = json!({"node_id":7,"run_id":"run","session_id":"session","turn_id":"old"});
    assert!(
        store.finish(&source, Ok(old_turn)).is_err(),
        "旧成果回合不能冒充新修改任务"
    );
    assert_eq!(
        store
            .get("phone", "feedback", "project", "owner")
            .expect("查询")
            .expect("预留")
            .state,
        "accepted",
        "不可信回执不能完成原操作"
    );
    let result = json!({"node_id":7,"run_id":"run","session_id":"session","turn_id":"new"});
    store
        .finish(&source, Ok(result.clone()))
        .expect("真实新回合");
    store.finish(&source, Ok(result)).expect("同回执幂等");
    let receipt = store
        .get("phone", "feedback", "project", "owner")
        .expect("读取")
        .expect("回执");
    assert_eq!(receipt.state, "succeeded", "准确新回合才表示Agent收到");
    let result = receipt.result.expect("新回合身份");
    assert_eq!(result["turn_id"], "new", "执行身份必须是新任务");
    assert_eq!(
        result["feedback"]["key"]["turn_id"], "old",
        "原成果身份仍是旧任务"
    );
    assert_eq!(
        result["feedback"]["fingerprint"], feedback.fingerprint,
        "回执保留原成果链接"
    );
    assert!(
        result["feedback"].get("text").is_none(),
        "身份回执不能混入完整意见正文"
    );
    assert_eq!(
        deliveries.get("project", &key).expect("旧成果"),
        Some(original),
        "反馈不能改写旧成果"
    );
    assert!(
        store
            .delivery_acceptance("project", &key)
            .expect("查验收")
            .is_none(),
        "提出修改不等于验收"
    );
}
