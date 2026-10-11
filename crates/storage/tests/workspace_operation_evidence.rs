#[path = "support/workspace_operations.rs"]
mod support;
use agent::card_instance::{CardInstanceEvent, CardInstanceStage, CardInstanceTurn};
use agent::workspace_operation::WorkspaceOperationOrigin;
use storage::{MobileAccessStore, MobilePermission, WorkspaceOperationAuthorization};
use support::{setup, source};

/// writer重试相同事件须幂等，输入阶段与等待原因均保存，可信终态后迟到queued证据不能覆盖。
/// 数据构造：已接收两项，第一项Armed；人工构造同target/session/turn的提交、运行和成功证据。
/// 执行过程：写waiting正文事件、同事件重写、按证据晋升、再写旧Accepted。
/// 预期结果：重复cursor不变，body/Enter阶段准确，旧事件不回退成功且历史项目/卡名仍冻结。
#[test]
fn event_retry_is_idempotent_and_late_snapshot_cannot_regress_terminal() {
    let (_temp, store, request) = setup();
    store.accept(&request).expect("应接收整组");
    store.arm_input("assignment-a").expect("应armed");
    let mut receipt = store
        .assignment("assignment-a")
        .expect("应查询")
        .expect("原实例存在")
        .receipt;
    let accepted = receipt.clone();
    receipt.stage = CardInstanceStage::WaitingInput;
    receipt.waiting_reason = Some("draft_pasted_enter_pending".into());
    let draft = CardInstanceEvent {
        receipt: receipt.clone(),
        observed_at_ms: 10,
        evidence: "draft_pasted_enter_pending".into(),
    };
    store
        .record_events(std::slice::from_ref(&draft))
        .expect("应保存正文阶段");
    let cursor = store
        .receipt(&source())
        .expect("应查询")
        .expect("存在")
        .cursor;
    store.record_events(&[draft]).expect("writer重复应安全");
    assert_eq!(
        store
            .receipt(&source())
            .expect("查询重复")
            .expect("存在")
            .cursor,
        cursor,
        "同证据不能重复生成事件"
    );
    receipt.stage = CardInstanceStage::Submitted;
    receipt.waiting_reason = None;
    store
        .record_events(&[CardInstanceEvent {
            receipt: receipt.clone(),
            observed_at_ms: 11,
            evidence: "enter_confirmed".into(),
        }])
        .expect("Enter证据应提交");
    receipt.stage = CardInstanceStage::Running;
    receipt.bound_turn = Some(CardInstanceTurn {
        run_id: Some("run-a".into()),
        session_id: "session-a".into(),
        turn_id: "turn-a".into(),
    });
    store
        .record_events(&[CardInstanceEvent {
            receipt: receipt.clone(),
            observed_at_ms: 12,
            evidence: "provider_turn_started".into(),
        }])
        .expect("精确start应运行");
    receipt.stage = CardInstanceStage::Succeeded;
    store
        .record_events(&[CardInstanceEvent {
            receipt,
            observed_at_ms: 13,
            evidence: "provider_turn_terminal".into(),
        }])
        .expect("精确terminal应完成");
    store
        .record_events(&[CardInstanceEvent {
            receipt: accepted,
            observed_at_ms: 14,
            evidence: "late_snapshot".into(),
        }])
        .expect("迟到旧事实应忽略");
    let persisted = store
        .assignment("assignment-a")
        .expect("查询最终")
        .expect("存在");
    assert_eq!(
        persisted.receipt.stage,
        CardInstanceStage::Succeeded,
        "旧排队快照不得覆盖终态"
    );
    assert_eq!(
        persisted.input_phase,
        storage::WorkspaceInputPhase::EnterConfirmed,
        "独立输入阶段必须有Enter证据"
    );
    assert_eq!(
        persisted.assignment.name_at_acceptance, "Alan",
        "历史名快照应保留"
    );
    assert_eq!(persisted.project_name, "项目甲", "历史项目不能依赖区域DB");
}

/// 未armed时提交与无原turn绑定的Unknown不能通过伪终态完成。
/// 数据构造：新接收A，仅把receipt改Submitted；随后armed并restart变Unknown但仍无bound turn。
/// 执行过程：先写未armed提交，再为Unknown伪造session相同的新turn成功证据。
/// 预期结果：两者均拒绝，原任务仍待核对且不能安全重试。
#[test]
fn unarmed_input_and_unbound_unknown_terminal_evidence_are_rejected() {
    let (_temp, store, request) = setup();
    store.accept(&request).expect("应接收");
    let mut receipt = store
        .assignment("assignment-a")
        .expect("应查询")
        .expect("存在")
        .receipt;
    receipt.stage = CardInstanceStage::Submitted;
    assert!(
        store
            .record_events(&[CardInstanceEvent {
                receipt,
                observed_at_ms: 10,
                evidence: "enter_confirmed".into()
            }])
            .is_err(),
        "未armed不能声称输入提交"
    );
    store.arm_input("assignment-a").expect("应armed");
    store.reconcile_interrupted().expect("重启应待核对");
    let mut receipt = store
        .assignment("assignment-a")
        .expect("应查询")
        .expect("存在")
        .receipt;
    receipt.stage = CardInstanceStage::Succeeded;
    receipt.failure = None;
    receipt.bound_turn = Some(CardInstanceTurn {
        run_id: Some("run-a".into()),
        session_id: "session-a".into(),
        turn_id: "guessed-turn".into(),
    });
    assert!(
        store
            .record_events(&[CardInstanceEvent {
                receipt,
                observed_at_ms: 11,
                evidence: "provider_turn_terminal".into()
            }])
            .is_err(),
        "未绑定Unknown不得按猜测turn补完成"
    );
    assert_eq!(
        store
            .assignment("assignment-a")
            .expect("应查询原状态")
            .expect("存在")
            .receipt
            .stage,
        CardInstanceStage::OutcomeUnknown,
        "拒绝后保留原待核对"
    );
}

/// 手机授权必须在atomic accept及输入前事务复核，credential不能被持久化。
/// 数据构造：仅Read的手机和两张角色，随后明确提升Control/revision再接收。
/// 执行过程：Read尝试派工被拒绝，Control接收，然后revoke并尝试Armed。
/// 预期结果：Read无operation，revoke后零Armed；保存source不含token。
#[test]
fn mobile_read_cannot_accept_and_revocation_before_arm_prevents_input() {
    let (temp, store, mut request) = setup();
    let access = MobileAccessStore::new(temp.path().join("app.sqlite"));
    let credential = "a".repeat(64);
    let grant = access
        .register_all_projects("phone-a", "测试手机", &credential, MobilePermission::Read)
        .expect("应配对read手机");
    request.source.origin = WorkspaceOperationOrigin::Mobile {
        device_id: "phone-a".into(),
    };
    request.authorization = WorkspaceOperationAuthorization::Mobile {
        device_id: "phone-a".into(),
        revision: grant.revision,
        authorized_area_ids: vec!["area-a".into(), "area-b".into()],
    };
    assert!(store.accept(&request).is_err(), "Read不能派工");
    assert_eq!(
        store.receipt(&request.source).expect("查询read尝试"),
        None,
        "无Control不能创建部分记录"
    );
    let control = access
        .replace_with_all_projects("phone-a", grant.revision, MobilePermission::Control)
        .expect("应明确提升Control");
    request.authorization = WorkspaceOperationAuthorization::Mobile {
        device_id: "phone-a".into(),
        revision: control.revision,
        authorized_area_ids: vec!["area-a".into(), "area-b".into()],
    };
    store.accept(&request).expect("Control应接收");
    access.revoke("phone-a").expect("应撤权");
    assert!(
        store.arm_input("assignment-a").is_err(),
        "撤权后输入前事务应拒绝"
    );
    let receipt = store
        .receipt(&request.source)
        .expect("后台历史查询")
        .expect("接受事实应保留");
    assert_eq!(
        receipt.assignments[0].input_phase,
        storage::WorkspaceInputPhase::Queued,
        "撤权不得留下Armed意图"
    );
    assert!(
        !serde_json::to_string(&receipt)
            .expect("应序列化回执")
            .contains(&credential),
        "secret不属于source/ledger/result DTO"
    );
}

/// 已写正文的attempt不能借旧WaitingInput阶段伪造“零输入取消”。
/// 数据构造：A已Armed并记录draft_pasted_enter_pending，phase为BodyWritten而stage仍WaitingInput。
/// 执行过程：提交无bound turn的Cancelled，再读取原phase与stage。
/// 预期结果：没有Provider终态证明的取消被拒绝，正文事实保留，不能称为未输入取消。
#[test]
fn cancellation_without_turn_cannot_claim_zero_input_after_body_written() {
    let (_temp, store, request) = setup();
    store.accept(&request).expect("应接收");
    store.arm_input("assignment-a").expect("应armed");
    let mut receipt = store
        .assignment("assignment-a")
        .expect("应查询")
        .expect("A存在")
        .receipt;
    receipt.stage = CardInstanceStage::WaitingInput;
    receipt.waiting_reason = Some("draft_pasted_enter_pending".into());
    store
        .record_events(&[CardInstanceEvent {
            receipt: receipt.clone(),
            observed_at_ms: 10,
            evidence: "draft_pasted_enter_pending".into(),
        }])
        .expect("应持久正文已写事实");
    receipt.stage = CardInstanceStage::Cancelled;
    receipt.waiting_reason = None;
    assert!(
        store
            .record_events(&[CardInstanceEvent {
                receipt,
                observed_at_ms: 11,
                evidence: "cancelled_before_input".into()
            }])
            .is_err(),
        "已写正文不能用WaitingInput伪造零输入取消"
    );
    let original = store
        .assignment("assignment-a")
        .expect("读拒绝后")
        .expect("A存在");
    assert_eq!(
        original.input_phase,
        storage::WorkspaceInputPhase::BodyWritten,
        "正文事实不能清除"
    );
    assert_eq!(
        original.receipt.stage,
        CardInstanceStage::WaitingInput,
        "拒绝不能伪造已取消终态"
    );
}
