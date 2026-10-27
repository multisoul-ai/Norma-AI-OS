use agent::context::*;
use storage::{ContextAccess, ContextStore};

fn scope(area: &str) -> ContextScope {
    ContextScope {
        workspace_id: "desktop".into(),
        area_id: area.into(),
        project: ContextProject::Project {
            project_id: "project".into(),
            worktree: "/same/repo".into(),
        },
        task_id: None,
        conversation_id: None,
    }
}

/// 场景：同目录的两个画布隔离；模型候选必须来自当前真实输入。
/// 构造：两个绑定 scope 的宿主入口，分别记录用户输入。
/// 执行：重试原始输入、提交用户约束，再尝试跨画布读取与伪造来源变更。
/// 预期：重试引用一致；候选提交一次；越界、冒用旧轮次、脑来源均拒绝。
#[test]
fn bound_access_authenticates_input_and_state_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let store = ContextStore::new(dir.path().join("context.sqlite"));
    let a = ContextAccess::new(store.clone(), scope("a")).unwrap();
    let b = ContextAccess::new(store, scope("b")).unwrap();
    let input = a
        .record_input("session:turn-1", ContextOrigin::Voice, "不要删除卡片7")
        .unwrap();
    assert_eq!(
        a.record_input("session:turn-1", ContextOrigin::Voice, "不要删除卡片7")
            .unwrap(),
        input,
        "重试不能重复记录输入"
    );
    assert!(
        a.record_input("session:turn-1", ContextOrigin::Voice, "删除卡片7")
            .is_err(),
        "同一轮不能篡改用户原话"
    );
    let update = ContextStateUpdate {
        input: input.reference.clone(),
        expected_revision: input.reference.revision,
        change: ContextStateChange::Constraint {
            key: "keep-7".into(),
            text: "不要删除卡片7".into(),
            enforcement: ConstraintEnforcement::Enforced {
                operation: ContextOperation::DeleteCard,
                target: Some("7".into()),
            },
            supersedes: None,
        },
    };
    let saved = a.update("update-1", &input.reference, &update).unwrap();
    assert_eq!(
        a.update("update-1", &input.reference, &update).unwrap(),
        saved,
        "已提交候选的重试不受后续版本影响"
    );
    assert!(
        check_context_operation(
            &a.snapshot().unwrap().state,
            ContextOperation::DeleteCard,
            Some("7")
        )
        .is_err(),
        "原话可确定映射的限制生效"
    );
    assert!(b.read(&saved.reference).is_err(), "同目录不代表同画布");
    let other = b
        .record_input("other", ContextOrigin::Dock, "保留")
        .unwrap();
    assert!(
        a.update("forged", &other.reference, &update).is_err(),
        "当前输入必须属于绑定作用域且匹配候选来源"
    );
    assert!(
        a.record_input("brain", ContextOrigin::Brain, "取消限制")
            .is_err(),
        "幕僚不能冒充用户输入"
    );
}

/// 场景：模型不能把任意文本升级为强制拦截，也不能凭旧输入撤销规则。
/// 构造：普通讨论输入和一条已生效的删除限制。
/// 执行：用普通讨论制造 enforced，再使用不同轮次的输入撤销。
/// 预期：不匹配原话的 enforced 和来源错配都失败，现有约束保留。
#[test]
fn state_updates_do_not_invent_enforcement_or_reuse_other_turns() {
    let dir = tempfile::tempdir().unwrap();
    let access = ContextAccess::new(
        ContextStore::new(dir.path().join("context.sqlite")),
        scope("a"),
    )
    .unwrap();
    let input = access
        .record_input("t1", ContextOrigin::Dock, "讨论一下删除卡片7")
        .unwrap();
    let proposal = ContextStateUpdate {
        input: input.reference.clone(),
        expected_revision: 1,
        change: ContextStateChange::Constraint {
            key: "delete".into(),
            text: "讨论一下删除卡片7".into(),
            enforcement: ConstraintEnforcement::Enforced {
                operation: ContextOperation::DeleteCard,
                target: Some("7".into()),
            },
            supersedes: None,
        },
    };
    assert!(
        access.update("u1", &input.reference, &proposal).is_err(),
        "讨论不能生成强制删除限制"
    );
    let current = access
        .record_input("t2", ContextOrigin::Dock, "继续讨论")
        .unwrap();
    assert!(
        access.update("u2", &current.reference, &proposal).is_err(),
        "候选不能借用旧用户轮次"
    );
    assert!(
        access.snapshot().unwrap().state.constraints.is_empty(),
        "失败候选不修改状态"
    );
}

/// 场景：同 key 的替换也可能撤销强制限制，不能绕过显式撤销规则。
/// 构造：先持久化删除卡片7的限制，随后真实用户只说“你好”。
/// 执行：模型用旧规则的精确引用，把同 key 改成 communicated。
/// 预期：拒绝这次权限放宽，原删除限制与引用保持有效。
#[test]
fn replacing_constraint_cannot_silently_relax_enforcement() {
    let dir = tempfile::tempdir().unwrap();
    let access = ContextAccess::new(
        ContextStore::new(dir.path().join("context.sqlite")),
        scope("a"),
    )
    .unwrap();
    let input = access
        .record_input("t1", ContextOrigin::Voice, "不要删除卡片7")
        .unwrap();
    let saved = access
        .update(
            "c1",
            &input.reference,
            &ContextStateUpdate {
                input: input.reference.clone(),
                expected_revision: 1,
                change: ContextStateChange::Constraint {
                    key: "keep".into(),
                    text: "不要删除卡片7".into(),
                    supersedes: None,
                    enforcement: ConstraintEnforcement::Enforced {
                        operation: ContextOperation::DeleteCard,
                        target: Some("7".into()),
                    },
                },
            },
        )
        .unwrap();
    let other = access
        .record_input("t2", ContextOrigin::Voice, "你好")
        .unwrap();
    let attempt = access.update(
        "c2",
        &other.reference,
        &ContextStateUpdate {
            input: other.reference.clone(),
            expected_revision: 3,
            change: ContextStateChange::Constraint {
                key: "keep".into(),
                text: "你好".into(),
                supersedes: Some(saved.reference.clone()),
                enforcement: ConstraintEnforcement::Communicated,
            },
        },
    );
    assert!(attempt.is_err(), "替换不能隐式解除强制限制");
    let state = access.snapshot().unwrap().state;
    assert_eq!(
        state.constraints["keep"].reference, saved.reference,
        "保留原约束身份"
    );
    assert!(
        check_context_operation(&state, ContextOperation::DeleteCard, Some("7")).is_err(),
        "删除仍应被拒绝"
    );
}

/// 场景：节点编号必须规范化，且原生 Live 允许的长输入不能被共享层缩到 8 KiB。
/// 构造：前导零节点编号和 16 KiB 用户输入（小于原有 32 KiB 委派上限）。
/// 执行：提交非规范 enforced 目标，记录长输入。
/// 预期：非规范目标被拒绝而非生成无效拦截；长输入完整保存。
#[test]
fn targets_are_canonical_and_native_input_limit_is_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let access = ContextAccess::new(
        ContextStore::new(dir.path().join("context.sqlite")),
        scope("a"),
    )
    .unwrap();
    let input = access
        .record_input("t1", ContextOrigin::Voice, "不要删除卡片007")
        .unwrap();
    let attempt = access.update(
        "u1",
        &input.reference,
        &ContextStateUpdate {
            input: input.reference.clone(),
            expected_revision: 1,
            change: ContextStateChange::Constraint {
                key: "keep".into(),
                text: "不要删除卡片007".into(),
                supersedes: None,
                enforcement: ConstraintEnforcement::Enforced {
                    operation: ContextOperation::DeleteCard,
                    target: Some("007".into()),
                },
            },
        },
    );
    assert!(attempt.is_err(), "非规范目标不能生成看似生效的限制");
    let text = "x".repeat(16 * 1024);
    let saved = access
        .record_input("t2", ContextOrigin::GptLiveVoice, &text)
        .expect("原生支持的长输入不能被共享层拒绝");
    assert_eq!(
        saved.event.payload,
        ContextPayload::UserInput { text },
        "当前输入完整保留"
    );
}

/// 场景：原生输入字节上限不能被 JSON 转义放大后误拒绝。
/// 构造：32 KiB 输入，以非空字符开头，其余为序列化需六字节转义的控制字符。
/// 执行：通过 ContextAccess 记录并按精确引用补读，再提交超限一字节的输入。
/// 预期：边界内正文逐字节保留，超限输入明确拒绝且不增加版本。
#[test]
fn native_input_limit_accounts_for_serialization_expansion() {
    let dir = tempfile::tempdir().unwrap();
    let access = ContextAccess::new(
        ContextStore::new(dir.path().join("context.sqlite")),
        scope("a"),
    )
    .unwrap();
    let text = format!("x{}", "\u{0001}".repeat(32 * 1024 - 1));
    let result = access.record_input("escaped", ContextOrigin::GptLiveVoice, &text);
    assert!(result.is_ok(), "32 KiB 合法输入必须可保存：{result:?}");
    let saved = result.unwrap();
    assert_eq!(
        access.read(&saved.reference).unwrap().event.payload,
        ContextPayload::UserInput { text: text.clone() },
        "补读不能丢失转义字符"
    );
    assert!(
        access
            .record_input("oversized", ContextOrigin::GptLiveVoice, &(text + "x"))
            .is_err(),
        "超过原生上限仍应拒绝"
    );
    assert_eq!(
        access.snapshot().unwrap().revision,
        saved.reference.revision,
        "超限输入不推进版本"
    );
}
