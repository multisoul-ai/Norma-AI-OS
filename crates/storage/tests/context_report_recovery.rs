use agent::context::*;
use storage::{ContextAccess, ContextStore, StorageError};

/// 场景：恢复读取了 Playing 快照，但真实完成回执抢先提交。
/// 构造：带决定来源的报告及 Playing 快照，在恢复提交前写入 Completed。
/// 执行：用捕获的旧快照执行恢复，再以最新快照重试，并尝试另一个画布的快照。
/// 预期：CAS 拒绝覆盖完成回执，重试不增加版本；越 scope 拒绝，原来源链完整可读。
#[test]
fn report_recovery_cas_preserves_concurrent_terminal_receipt() {
    let dir = tempfile::tempdir().unwrap();
    let scope = ContextScope {
        workspace_id: "w".into(),
        area_id: "a".into(),
        project: ContextProject::Personal,
        task_id: None,
        conversation_id: None,
    };
    let access =
        ContextAccess::new(ContextStore::new(dir.path().join("context.sqlite")), scope).unwrap();
    let decision = access
        .host_event(
            "decision",
            ContextPayload::Decision {
                status: DecisionStatus::Denied,
                reason: "等待确认范围".into(),
            },
            vec![],
        )
        .unwrap()
        .reference;
    let report = access
        .report("report", "操作已拦下", vec![decision.clone()])
        .unwrap()
        .reference;
    access
        .host_event(
            "playing",
            ContextPayload::Delivery {
                report: report.clone(),
                status: ReportDelivery::Playing,
            },
            vec![report.clone()],
        )
        .unwrap();
    let stale = access.snapshot().unwrap();
    access
        .host_event(
            "completed",
            ContextPayload::Delivery {
                report: report.clone(),
                status: ReportDelivery::Completed,
            },
            vec![report.clone()],
        )
        .unwrap();
    let current = access.snapshot().unwrap();
    let error = access.recover_reports(&stale).unwrap_err();
    assert!(
        matches!(error, StorageError::InvalidContext(ref code) if code == "stale_snapshot"),
        "必须在提交时拒绝旧恢复版本，实际 {error:?}"
    );
    assert_eq!(
        access.snapshot().unwrap(),
        current,
        "失败恢复不得留下 Unknown 事件或回退 Completed"
    );
    access.recover_reports(&current).unwrap();
    assert_eq!(
        access.snapshot().unwrap().revision,
        current.revision,
        "最新终态不再恢复"
    );
    let mut foreign = stale;
    foreign.scope.area_id = "other".into();
    assert!(
        matches!(access.recover_reports(&foreign), Err(StorageError::InvalidContext(code)) if code == "scope_mismatch"),
        "恢复只能使用当前 scope 的快照"
    );
    assert_eq!(
        access.read(&report).unwrap().event.related,
        vec![decision.clone()],
        "恢复与回执竞争不改变报告的决定来源"
    );
    assert_eq!(
        access.read(&decision).unwrap().reference,
        decision,
        "决定原文引用仍可读"
    );
}
