use agent::context::*;

fn record(
    id: &str,
    revision: u64,
    origin: ContextOrigin,
    payload: ContextPayload,
) -> RecordedContextEvent {
    let scope = ContextScope {
        workspace_id: "w".into(),
        area_id: "a".into(),
        project: ContextProject::Personal,
        task_id: None,
        conversation_id: None,
    };
    RecordedContextEvent {
        reference: ContextRef {
            scope: scope.clone(),
            event_id: id.into(),
            revision,
        },
        event: ContextEvent {
            event_id: id.into(),
            scope,
            source: ContextSource {
                origin,
                stream_id: "s".into(),
                sequence: revision,
            },
            created_at_ms: 1,
            related: vec![],
            payload,
        },
    }
}

/// 场景：文本要求不冒充技术拦截，幕僚不能撤销用户限制。
/// 构造：用户的已沟通要求，以及宿主可核验的删除操作限制。
/// 执行：检查未受限/受限目标，再尝试幕僚撤销、旧引用撤销和用户精确撤销。
/// 预期：仅对应操作/目标被拒绝；幕僚或旧引用不解除限制；有效撤销只更新状态。
#[test]
fn constraints_distinguish_communication_enforcement_and_revocation() {
    let text = record(
        "text",
        1,
        ContextOrigin::Voice,
        ContextPayload::Constraint {
            key: "release".into(),
            text: "先别发布".into(),
            enforcement: ConstraintEnforcement::Communicated,
            supersedes: None,
        },
    );
    let state = project_event(&WorkingState::default(), &text).unwrap();
    assert!(
        check_context_operation(&state, ContextOperation::DeleteCard, Some("7")).is_ok(),
        "文本要求不能被伪装成另一类技术拦截"
    );
    let block = record(
        "block",
        2,
        ContextOrigin::Voice,
        ContextPayload::Constraint {
            key: "delete".into(),
            text: "保留卡片 7".into(),
            enforcement: ConstraintEnforcement::Enforced {
                operation: ContextOperation::DeleteCard,
                target: Some("7".into()),
            },
            supersedes: None,
        },
    );
    let state = project_event(&state, &block).unwrap();
    assert!(
        check_context_operation(&state, ContextOperation::DeleteCard, Some("7")).is_err(),
        "目标删除必须被限制"
    );
    assert!(
        check_context_operation(&state, ContextOperation::DeleteCard, Some("8")).is_ok(),
        "限制不能扩大到其他目标"
    );
    let mut revoke = record(
        "revoke",
        3,
        ContextOrigin::Brain,
        ContextPayload::RevokeConstraint {
            key: "delete".into(),
            supersedes: block.reference.clone(),
        },
    );
    assert!(
        project_event(&state, &revoke).is_err(),
        "幕僚不能自行撤销用户规则"
    );
    revoke.event.source.origin = ContextOrigin::Voice;
    let updated = project_event(&state, &revoke).unwrap();
    assert!(
        updated.constraints.contains_key("release"),
        "撤销不影响其他约束"
    );
    assert!(
        !updated.constraints.contains_key("delete"),
        "用户的精确撤销才生效"
    );
    assert!(
        project_event(&updated, &revoke).is_err(),
        "已失效引用不能再次撤销其他规则"
    );
}

/// 场景：坏目标或不一致的事件引用不能生成看似有效的限制。
/// 构造：空白目标的用户限制，再把目标修好但伪造 record 的画布。
/// 执行：通过公共投影入口尝试两次变更。
/// 预期：均返回错误，不能把错误引用带入工作状态。
#[test]
fn projection_rejects_invalid_targets_and_mismatched_event_identity() {
    let mut value = record(
        "block",
        1,
        ContextOrigin::Voice,
        ContextPayload::Constraint {
            key: "delete".into(),
            text: "不要删除".into(),
            enforcement: ConstraintEnforcement::Enforced {
                operation: ContextOperation::DeleteCard,
                target: Some(" ".into()),
            },
            supersedes: None,
        },
    );
    assert!(
        project_event(&WorkingState::default(), &value).is_err(),
        "空目标不能冒充可执行限制"
    );
    let ContextPayload::Constraint { enforcement, .. } = &mut value.event.payload else {
        panic!("约束夹具")
    };
    *enforcement = ConstraintEnforcement::Enforced {
        operation: ContextOperation::DeleteCard,
        target: Some("7".into()),
    };
    value.reference.scope.area_id = "other".into();
    assert!(
        project_event(&WorkingState::default(), &value).is_err(),
        "事件与引用身份不一致应失败"
    );
}
