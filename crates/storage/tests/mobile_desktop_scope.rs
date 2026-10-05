use storage::*;
use workspace::review::ProjectReview;

/// 桌面区域持久身份：交付与验收共享精确 scope，权限 ID 仍拒绝冒用复合身份。
/// 数据构造：desktop:space-a:area-a 与 area-b；节点 7、一个完成回合，空文件成果。
/// 执行过程：预留 → 起点绑定 → 后台终态提交（沿用 5 秒预算）→ 冻结 → 验收 → 查另一 scope。
/// 预期结果：本区域验收消项；另一区域无交付；复合设备 ID 与多段 scope 均拒绝。
#[test]
fn desktop_scope_keeps_delivery_and_acceptance_isolated() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("app.sqlite");
    let scope = "desktop:space-a:area-a";
    let source = NewMobileCommand {
        device_id: "phone".into(),
        operation_id: "task".into(),
        workspace_id: scope.into(),
        route_id: "route".into(),
        owner_id: "owner".into(),
        request_hash: "a".repeat(64),
    };
    let key = MobileTurnKey {
        node_id: 7,
        profile: "Codex".into(),
        run_id: "run".into(),
        session_id: "session".into(),
        turn_id: "turn".into(),
    };
    let commands = MobileCommandStore::new(db.clone());
    commands.reserve(&source).unwrap();
    let review = ProjectReview {
        base_commit: None,
        conflicted: false,
        files: vec![],
    };
    let baselines = MobileTaskBaselineStore::new(db.clone());
    baselines
        .record(
            &source,
            &key,
            &TaskBaseline::Available {
                review: review.clone(),
            },
        )
        .unwrap();
    assert_eq!(
        baselines
            .turn_for_command("phone", "task", scope, "route")
            .unwrap(),
        Some(key.clone()),
        "原操作应绑定准确区域回合"
    );
    let writer = BackgroundApprovalAudit::new(
        ApprovalAuditStore::new(dir.path().join("audit.sqlite"))
            .with_turn_store(MobileTurnStore::for_workspace(db.clone(), scope.into())),
    )
    .unwrap();
    // 5 秒为现有审计 writer 的提交等待预算。
    writer
        .record_turn(MobileTurnOutcome {
            key: key.clone(),
            state: MobileTurnState::Completed,
            summary: None,
        })
        .unwrap()
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap()
        .unwrap();
    let deliveries = MobileTaskDeliveryStore::new(db);
    let saved = deliveries
        .record(scope, &key, &TaskDeliveryContent::Available { review })
        .unwrap();
    assert_eq!(
        deliveries.page(scope, None, None, true).unwrap().1,
        1,
        "完成且可读成果计入待验收"
    );
    let mut acceptance = source.clone();
    acceptance.operation_id = "accept".into();
    commands.reserve(&acceptance).unwrap();
    commands
        .accept_delivery(&acceptance, &key, &saved.fingerprint)
        .unwrap();
    assert_eq!(
        deliveries.page(scope, None, None, true).unwrap().1,
        0,
        "验收应移除待处理计数"
    );
    assert!(
        deliveries
            .get("desktop:space-a:area-b", &key)
            .unwrap()
            .is_none(),
        "同卡片ID在其他区域不可冒领成果"
    );
    let mut bad = source.clone();
    bad.device_id = scope.into();
    assert!(
        commands.reserve(&bad).is_err(),
        "授权设备 ID 不允许复合 scope"
    );
    assert!(
        deliveries
            .get("desktop:space-a:area-a:extra", &key)
            .is_err(),
        "多余段必须拒绝"
    );
}
