use super::*;
use agent::context::{ContextPayload, DecisionStatus, ReceiptStatus, ReportDelivery};
use gpui::TestAppContext;

/// 场景：幕僚把审批转交用户后，语音追问需要知道是在等确认，不能误报已拒绝或已执行。
/// 构造：隔离数据库、一条真实 tracked 审批，关闭播报但保留共享报告。
/// 执行：调用生产 escalate，等到报告保存与 Skipped，再沿报告引用读取决定和回执。
/// 预期：决定为 AwaitingConfirmation，转交回执为 Accepted；两者关联，报告保留全部来源。
#[test]
fn brain_report_context_escalation_links_waiting_decision_and_actual_receipt() {
    confirmation_fixture(short_target(), "发布范围尚未确认");
}

fn short_target() -> norma::mobile::ApprovalTarget {
    norma::mobile::ApprovalTarget {
        node_id: 7,
        profile: "Codex".into(),
        run_id: "run".into(),
        session_id: "session".into(),
        turn_id: "turn".into(),
        request_id: "request".into(),
    }
}

/// 场景：合法审批身份可超过回执的 256 字节上限，不能因此丢失确认提醒。
/// 构造：run/session/turn 各 128 字节、request 256 字节；两条审批仅末尾一个字节不同。
/// 执行：以生产 encode_id 和 escalate 生成报告并读取来源。
/// 预期：两条报告均保存，回执身份有界且不因截断长前缀碰撞。
#[test]
fn brain_report_context_escalation_accepts_long_approval_identity() {
    let mut target = short_target();
    target.run_id = "r".repeat(128);
    target.session_id = "s".repeat(128);
    target.turn_id = "t".repeat(128);
    target.request_id = "q".repeat(256);
    assert!(
        super::super::approvals::encode_id(&target).len() > 256,
        "合法长审批必须越过 256 字节边界"
    );
    let first = confirmation_fixture(target.clone(), "需要确认");
    assert_eq!(
        first,
        confirmation_fixture(target.clone(), "需要确认"),
        "同一完整审批身份应映射到稳定回执身份"
    );
    target.request_id.replace_range(255.., "z");
    let second = confirmation_fixture(target, "需要确认");
    assert_ne!(first, second, "不能通过截断共同前缀混淆不同审批");
}

/// 场景：协议允许空白理由，转交动作已发生，仍须保存用户可读的等待确认事实。
/// 构造：短合法审批，理由分别为空串和空白；不填造任何风险判断。
/// 执行：调用生产 escalate 并读取报告的决定来源。
/// 预期：保留 AwaitingConfirmation，理由明确为“未提供理由”，报告仍能保存。
#[test]
fn brain_report_context_escalation_preserves_missing_reason_as_missing() {
    for reason in ["", " \n\t"] {
        confirmation_fixture(short_target(), reason);
    }
}

fn confirmation_fixture(target: norma::mobile::ApprovalTarget, reason: &str) -> String {
    assert!(target.valid(), "夹具必须是产品接受的合法审批身份");
    let id = super::super::approvals::encode_id(&target);
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let store = storage::AppStore::initialize(&paths).unwrap();
    let mut test = TestAppContext::single();
    let (entity, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    let service = entity.update(cx, |app, cx| {
        app.brain.tracked.insert(
            id.clone(),
            super::super::Tracked {
                target,
                agent: "Alan".into(),
                subject: brain::risk::ApprovalSubject {
                    tool_name: "Bash".into(),
                    command: Some("deploy".into()),
                    file_path: None,
                    project_root: dir.path().to_path_buf(),
                },
                escalated: false,
                since: std::time::Instant::now(),
                reminded: false,
            },
        );
        let service = app.shared_context_service().unwrap();
        app.escalate(&id, reason, cx);
        assert!(
            app.brain.tracked[&id].escalated,
            "审批必须实际进入等待用户状态"
        );
        service
    });
    let runtime = crate::app::shared_tokio_runtime().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let snapshot = loop {
        cx.run_until_parked();
        let snapshot = runtime.block_on(service.snapshot()).unwrap();
        if snapshot
            .state
            .reports
            .values()
            .any(|report| report.delivery == ReportDelivery::Skipped)
        {
            break snapshot;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "转交后的报告必须有界保存"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    let report = snapshot
        .state
        .reports
        .values()
        .next()
        .unwrap()
        .reference
        .clone();
    let records = runtime
        .block_on(service.run(None, move |access| {
            let record = access.read(&report)?;
            let sources = record
                .event
                .related
                .iter()
                .map(|reference| access.read(reference))
                .collect::<storage::StorageResult<Vec<_>>>()?;
            Ok((record, sources))
        }))
        .unwrap();
    assert_eq!(records.1.len(), 2, "提醒须关联一个真实决定和一个转交回执");
    let decision = records
        .1
        .iter()
        .find(|record| matches!(record.event.payload, ContextPayload::Decision { .. }))
        .unwrap();
    assert_eq!(
        decision.event.payload,
        ContextPayload::Decision {
            status: DecisionStatus::AwaitingConfirmation,
            reason: if reason.trim().is_empty() {
                "未提供理由".into()
            } else {
                reason.into()
            },
        },
        "等待用户确认不能记录为拒绝或放行"
    );
    let receipt = records
        .1
        .iter()
        .find(|record| matches!(record.event.payload, ContextPayload::Receipt { .. }))
        .unwrap();
    let ContextPayload::Receipt {
        operation_id,
        status,
        summary,
    } = &receipt.event.payload
    else {
        unreachable!()
    };
    assert!(
        !operation_id.is_empty() && operation_id.len() <= 256,
        "合法审批应映射到有界回执身份"
    );
    assert!(
        summary.contains(&format!("approval:{id}")),
        "回执身份摘要化后仍须保留原审批标识供精确追溯"
    );
    if id.len() + "approval:".len() <= 256 {
        assert_eq!(
            operation_id,
            &format!("approval:{id}"),
            "短审批回执身份应精确保留"
        );
    }
    assert_eq!(
        *status,
        ReceiptStatus::Accepted,
        "转交已接收，不表示审批内的命令执行成功"
    );
    assert_eq!(
        receipt.event.related,
        vec![decision.reference.clone()],
        "实际回执应精确关联决定"
    );
    assert!(
        records.0.event.related.contains(&receipt.reference),
        "报告保留实际回执来源"
    );
    operation_id.clone()
}
