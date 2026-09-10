use std::{fs, path::Path, time::Duration};
use storage::{
    ApprovalAuditStore, BackgroundApprovalAudit, MobileCommandStore, MobileTaskDeliveryStore,
    MobileTurnKey, MobileTurnOutcome, MobileTurnState, MobileTurnStore, NewMobileCommand,
    TaskDeliveryContent,
};

// Only the explicit notification-ui fixture calls this; it never targets the user's App database.
pub fn advance(dir: &Path, workspace: &str) {
    let database = dir.join("app.sqlite");
    let deliveries = MobileTaskDeliveryStore::new(database.clone());
    let key = |turn: &str| MobileTurnKey {
        node_id: 1,
        profile: "Codex".into(),
        run_id: "notification-run".into(),
        session_id: "notification-session".into(),
        turn_id: turn.into(),
    };
    if dir.join("ui-delivery-seed").exists() && !dir.join("ui-delivery-ready").exists() {
        let writer = BackgroundApprovalAudit::new(
            ApprovalAuditStore::new(dir.join("delivery-audit.sqlite")).with_turn_store(
                MobileTurnStore::for_workspace(database.clone(), workspace.into()),
            ),
        )
        .expect("隔离终态writer");
        for turn in ["delivery-first", "delivery-second"] {
            writer
                .record_turn(MobileTurnOutcome {
                    key: key(turn),
                    state: MobileTurnState::Completed,
                })
                .expect("排队")
                .recv_timeout(Duration::from_secs(5)) // 5s沿用正式writer提交预算。
                .expect("回执")
                .expect("落盘");
            deliveries
                .record(
                    workspace,
                    &key(turn),
                    &TaskDeliveryContent::Available {
                        review: workspace::review::ProjectReview {
                            base_commit: None,
                            conflicted: false,
                            files: vec![],
                        },
                    },
                )
                .expect("真实冻结成果");
        }
        fs::write(dir.join("ui-delivery-ready"), b"").expect("准备完成");
    }
    if dir.join("ui-delivery-accept").exists() && !dir.join("ui-delivery-accepted").exists() {
        let saved = deliveries
            .get(workspace, &key("delivery-second"))
            .expect("读取")
            .expect("存在");
        let commands = MobileCommandStore::new(database);
        let source = NewMobileCommand {
            device_id: "fixture-writer".into(),
            operation_id: "accept-ui-delivery".into(),
            workspace_id: workspace.into(),
            route_id: "notification-route".into(),
            owner_id: "fixture".into(),
            request_hash: "a".repeat(64),
        };
        commands.reserve(&source).expect("正式预留");
        commands
            .accept_delivery(&source, &saved.outcome.key, &saved.fingerprint)
            .expect("正式验收存储");
        fs::write(dir.join("ui-delivery-accepted"), b"").expect("验收完成");
    }
}
