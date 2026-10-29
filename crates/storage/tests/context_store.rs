use agent::context::{
    ConstraintEnforcement, ContextEvent, ContextOrigin, ContextPayload, ContextProject,
    ContextScope, ContextSource,
};
use storage::ContextStore;

fn scope(area: &str) -> ContextScope {
    ContextScope {
        workspace_id: "w".into(),
        area_id: area.into(),
        project: ContextProject::Project {
            project_id: "p".into(),
            worktree: "/same".into(),
        },
        task_id: None,
        conversation_id: None,
    }
}

fn constraint(id: &str, sequence: u64, text: &str) -> ContextEvent {
    ContextEvent {
        event_id: id.into(),
        scope: scope("a"),
        source: ContextSource {
            origin: ContextOrigin::Voice,
            stream_id: "session-1".into(),
            sequence,
        },
        created_at_ms: 1000,
        related: vec![],
        payload: ContextPayload::Constraint {
            key: "release".into(),
            text: text.into(),
            enforcement: ConstraintEnforcement::Communicated,
            supersedes: None,
        },
    }
}

/// 场景：有效约束必须与事件一起持久化；重试不能覆盖记录或增加版本。
/// 构造：临时真实 SQLite、同目录双画布、同 ID 的相同/不同正文。
/// 执行：提交、重复提交、冲突提交，重新打开并读取原画布及另一个画布。
/// 预期：首次提交版本为 1；重复提交幂等；冲突失败；重启仍只有一条约束且不串画布。
#[test]
fn durable_state_is_idempotent_and_does_not_cross_areas() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.sqlite");
    let store = ContextStore::new(path.clone());
    let event = constraint("e1", 1, "先不要发布");
    let first = store.append(&event, Some(0)).unwrap();
    assert_eq!(first.reference.revision, 1, "首次提交分配版本 1");
    assert_eq!(
        store.append(&event, Some(0)).unwrap(),
        first,
        "同一请求重试应返回原回执"
    );
    assert!(
        store
            .append(&constraint("e1", 1, "可以发布"), Some(1))
            .is_err(),
        "同 ID 不同内容必须失败"
    );
    drop(store);
    let reopened = ContextStore::new(path);
    let state = reopened.snapshot(&scope("a"), 20).unwrap();
    assert_eq!(state.revision, 1, "幂等重试和失败均不推进版本");
    assert_eq!(
        state.state.constraints["release"].text, "先不要发布",
        "已提交的约束必须可恢复"
    );
    assert_eq!(state.events.len(), 1, "事件与状态一同提交，不能重复");
    assert!(
        reopened
            .snapshot(&scope("b"), 20)
            .unwrap()
            .state
            .constraints
            .is_empty(),
        "同目录另一画布没有这些约束"
    );
}

/// 场景：较旧输入和错误撤销关系不能覆盖用户当前约束。
/// 构造：先写序号 2 的约束，再尝试序号 1、过期 revision、缺少 supersedes 的修订。
/// 执行：各失败请求后提交带正确旧引用的修订，最后读取最新状态。
/// 预期：仅正确修订推进版本；原事件仍可按引用读取，错误 area/version 读取失败。
#[test]
fn revision_and_source_order_guard_constraint_revisions() {
    let dir = tempfile::tempdir().unwrap();
    let store = ContextStore::new(dir.path().join("app.sqlite"));
    let first = store
        .append(&constraint("e1", 2, "先不要发布"), Some(0))
        .unwrap();
    assert!(
        store
            .append(&constraint("late", 1, "发布"), Some(1))
            .is_err(),
        "迟到的旧输入不得覆盖新约束"
    );
    assert!(
        store
            .append(&constraint("e2", 3, "只发布测试环境"), Some(0))
            .is_err(),
        "过期版本不能更新状态"
    );
    let mut update = constraint("e2", 3, "只发布测试环境");
    assert!(
        store.append(&update, Some(1)).is_err(),
        "替换已有约束必须明确旧引用"
    );
    let ContextPayload::Constraint { supersedes, .. } = &mut update.payload else {
        panic!("约束夹具")
    };
    *supersedes = Some(first.reference.clone());
    let second = store.append(&update, Some(1)).unwrap();
    assert_eq!(second.reference.revision, 2, "失败写入不能消耗版本");
    assert_eq!(
        store.snapshot(&scope("a"), 20).unwrap().state.constraints["release"].text,
        "只发布测试环境",
        "正确修订成为有效状态"
    );
    assert_eq!(
        store.read(&scope("a"), &first.reference).unwrap(),
        first,
        "旧事件保留，引用不指向新正文"
    );
    assert!(
        store.read(&scope("b"), &first.reference).is_err(),
        "引用读取不能扩大作用域"
    );
    let mut forged = first.reference.clone();
    forged.revision = 2;
    assert!(
        store.read(&scope("a"), &forged).is_err(),
        "错误引用版本不能读取"
    );
}
