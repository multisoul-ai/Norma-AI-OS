use agent::context::*;
use storage::ContextStore;

fn input(id: &str, sequence: u64, payload: ContextPayload) -> ContextEvent {
    ContextEvent {
        event_id: id.into(),
        scope: ContextScope {
            workspace_id: "w".into(),
            area_id: "a".into(),
            project: ContextProject::Personal,
            task_id: None,
            conversation_id: None,
        },
        source: ContextSource {
            origin: ContextOrigin::Dock,
            stream_id: "dock-1".into(),
            sequence,
        },
        created_at_ms: 1000,
        related: vec![],
        payload,
    }
}

/// 场景：事务中途写 checkpoint 失败，不能留下“已保存”的事件或状态。
/// 构造：先保存目标，再在真实 SQLite checkpoint 更新处安装失败触发器。
/// 执行：修订目标失败后移除故障，重开 store 检查，再重试同事件。
/// 预期：失败后仍是原目标/版本；重试成功只推进一次，不重放任何动作。
#[test]
fn checkpoint_failure_rolls_back_event_and_can_be_retried() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.sqlite");
    let store = ContextStore::new(path.clone());
    let first = input(
        "goal-1",
        1,
        ContextPayload::Goal {
            text: "先完成测试".into(),
            supersedes: None,
        },
    );
    let saved = store.append(&first, Some(0)).unwrap();
    let update = input(
        "goal-2",
        2,
        ContextPayload::Goal {
            text: "先修复失败测试".into(),
            supersedes: Some(saved.reference),
        },
    );
    let injection = rusqlite::Connection::open(&path).unwrap();
    injection.execute_batch("CREATE TRIGGER fail_context_checkpoint BEFORE UPDATE ON shared_context_state BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
    assert!(
        store.append(&update, Some(1)).is_err(),
        "checkpoint 故障必须让整个提交失败"
    );
    injection
        .execute_batch("DROP TRIGGER fail_context_checkpoint;")
        .unwrap();
    drop(injection);
    drop(store);
    let reopened = ContextStore::new(path);
    let snapshot = reopened.snapshot(&first.scope, 20).unwrap();
    assert_eq!(snapshot.revision, 1, "失败事务不能推进版本");
    assert_eq!(
        snapshot.state.goal.unwrap().text,
        "先完成测试",
        "原状态应完整保留"
    );
    assert_eq!(snapshot.events.len(), 1, "失败事务不能留下事件");
    assert_eq!(
        reopened
            .append(&update, Some(1))
            .unwrap()
            .reference
            .revision,
        2,
        "故障解除后同 ID 可安全重试"
    );
    assert_eq!(
        reopened
            .snapshot(&first.scope, 20)
            .unwrap()
            .state
            .goal
            .unwrap()
            .text,
        "先修复失败测试",
        "新目标应持久化"
    );
}
