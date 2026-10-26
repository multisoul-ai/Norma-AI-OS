use super::*;
use agent::{AdapterCommand, AdapterResult, AgentCardAdapter, ToolContext};
use futures::future::BoxFuture;

fn scope() -> ContextScope {
    ContextScope {
        workspace_id: "w".into(),
        area_id: "a".into(),
        project: ContextProject::Personal,
        task_id: None,
        conversation_id: None,
    }
}

/// 场景：另一个消费者刷新同 scope 的只读快照，不应中断已就绪的工具。
/// 构造：初始加载完成，后台读取用两个通道固定在读取开始后的时刻。
/// 执行：读取未完成时执行已就绪操作，再释放读取并等候收尾。
/// 预期：只读刷新期间仍可执行，读取结束 revision 不变。
#[tokio::test]
async fn shared_context_read_refresh_does_not_close_ready_execution_gate() {
    let dir = tempfile::tempdir().unwrap();
    let service = SharedContext::new(dir.path().join("app.sqlite"), scope()).unwrap();
    let before = service.snapshot().await.unwrap();
    let (began, started) = tokio::sync::oneshot::channel();
    let began = Mutex::new(Some(began));
    let (release, wait) = std::sync::mpsc::channel();
    let wait = Mutex::new(wait);
    let reader = service.clone();
    let task = tokio::spawn(async move {
        reader
            .run(None, move |access| {
                if let Some(began) = began.lock().unwrap().take() {
                    began.send(()).unwrap();
                }
                wait.lock().unwrap().recv().unwrap();
                access.snapshot()
            })
            .await
    });
    started.await.unwrap();
    let action = service.execute(ContextOperation::DispatchTask, None, || Ok("accepted"));
    release.send(()).unwrap();
    let after = task.await.unwrap().unwrap();
    assert_eq!(
        action.unwrap(),
        "accepted",
        "只读刷新不能使已就绪的执行误拒绝"
    );
    assert_eq!(before.revision, after.revision, "只读快照不得变更持久状态");
}
fn context(text: &str, call: &str) -> ToolContext {
    ToolContext {
        project_id: "p".into(),
        turn_id: "t1".into(),
        user_input: text.into(),
        input_source: agent::AgentInputSource::Voice,
        contextual_delete_targets: vec![],
        tool_call_id: call.into(),
        operation_id: None,
        cancellation: tokio_util::sync::CancellationToken::new(),
    }
}
struct Checkpoint(&'static str);
impl AgentCardAdapter for Checkpoint {
    fn execute<'a>(
        &'a self,
        _: AdapterCommand,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            let mut result = AdapterResult::succeeded("checkpoint");
            result.status = self.0.into();
            Ok(result)
        })
    }
}

/// 场景：切离再返回同画布时，旧适配器可能仍在完成已接受的状态写入。
/// 构造：同一数据库与 scope 创建两个宿主服务，先让两者读取空状态。
/// 执行：旧实例保存删除限制，新实例尝试实际执行。
/// 预期：相同 scope 共用写入与执行协调，另一个实例不能用旧快照放行。
#[tokio::test]
async fn shared_context_instances_share_the_same_execution_gate() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.sqlite");
    let old = SharedContext::new(path.clone(), scope()).unwrap();
    let current = SharedContext::new(path, scope()).unwrap();
    old.snapshot().await.unwrap();
    current.snapshot().await.unwrap();
    old.run(Some("restriction".into()), |access| {
        let input = access.record_input("t1", ContextOrigin::Voice, "不要删除卡片7")?;
        access.update(
            "u1",
            &input.reference,
            &ContextStateUpdate {
                input: input.reference.clone(),
                expected_revision: input.reference.revision,
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
    })
    .await
    .unwrap();
    assert!(
        current
            .execute(ContextOperation::DeleteCard, Some("7"), || Ok(()))
            .is_err(),
        "同 scope 的另一实例不能使用过期 gate"
    );
    assert!(
        current
            .execute(ContextOperation::DeleteCard, Some("8"), || Ok(()))
            .is_ok(),
        "相同实例协调不扩大限制范围"
    );
}

/// 场景：回执通道断开或只接收排队不能证明当前来源仍有效。
/// 构造：返回 unknown/accepted/rejected 的宿主 checkpoint。
/// 执行：通过正式适配器获取当前共享资料。
/// 预期：拒绝读取和记录输入；仅明确 succeeded 才可继续。
#[tokio::test]
async fn shared_context_requires_confirmed_checkpoint() {
    let dir = tempfile::tempdir().unwrap();
    let service = SharedContext::new(dir.path().join("app.sqlite"), scope()).unwrap();
    for status in ["unknown", "accepted", "rejected"] {
        let adapter = adapter::wrap(Arc::new(Checkpoint(status)), service.clone());
        assert!(
            adapter.shared_context(&context("你好", "c")).await.is_err(),
            "{status} 不能冒充来源核验成功"
        );
    }
    assert_eq!(
        service.snapshot().await.unwrap().revision,
        0,
        "核验失败不能写输入"
    );
    let adapter = adapter::wrap(Arc::new(Checkpoint("succeeded")), service);
    assert!(
        adapter
            .shared_context(&context("你好", "c"))
            .await
            .unwrap()
            .is_some(),
        "明确成功才读取资料"
    );
}

/// 场景：状态保存遇到 SQLite 故障，下一次模型调用会生成不同的 tool_call_id。
/// 构造：真实临时库、已记录的限制原话，以及 checkpoint 更新失败触发器。
/// 执行：更新失败，移除触发器，用新的 call ID 重试同一候选。
/// 预期：宿主恢复候选并只提交一次，删除限制生效，新输入仍能继续。
#[tokio::test]
async fn shared_context_recovers_save_without_original_tool_call() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.sqlite");
    let service = SharedContext::new(path.clone(), scope()).unwrap();
    let adapter = adapter::wrap(Arc::new(Checkpoint("succeeded")), service.clone());
    let first = context("不要删除卡片7", "first");
    let frame = adapter.shared_context(&first).await.unwrap().unwrap();
    let update = ContextStateUpdate {
        input: frame.input,
        expected_revision: frame.snapshot.revision,
        change: ContextStateChange::Constraint {
            key: "keep".into(),
            text: first.user_input.clone(),
            supersedes: None,
            enforcement: ConstraintEnforcement::Enforced {
                operation: ContextOperation::DeleteCard,
                target: Some("7".into()),
            },
        },
    };
    let db = rusqlite::Connection::open(path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_context_checkpoint BEFORE UPDATE ON shared_context_state BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(
        adapter
            .execute(
                AdapterCommand::UpdateWorkingState(Box::new(update.clone())),
                &first
            )
            .await
            .is_err(),
        "故障不能报告保存成功"
    );
    assert!(
        service
            .execute(ContextOperation::DeleteCard, Some("8"), || Ok(()))
            .is_err(),
        "保存未决期间暂停执行"
    );
    db.execute_batch("DROP TRIGGER fail_context_checkpoint;")
        .unwrap();
    let next = context("不要删除卡片7", "different-call");
    assert_eq!(
        adapter
            .execute(AdapterCommand::UpdateWorkingState(Box::new(update)), &next)
            .await
            .unwrap()
            .status,
        "succeeded",
        "不依赖原 call ID 恢复"
    );
    let saved = service.snapshot().await.unwrap();
    assert_eq!(saved.revision, 2, "原话和限制各提交一次");
    assert!(
        service
            .execute(ContextOperation::DeleteCard, Some("7"), || Ok(()))
            .is_err(),
        "保存限制必须拦截"
    );
    assert!(
        service
            .execute(ContextOperation::DeleteCard, Some("8"), || Ok(()))
            .is_ok(),
        "恢复后其他操作可继续"
    );
}

/// 场景：请求等待者被取消，后台 SQLite 提交仍必须完成 gate 收尾。
/// 构造：带开始/释放通道的后台写入，在开始后取消等待者。
/// 执行：释放后台写入，再通过同服务读取。
/// 预期：事实只写一次，gate 恢复，不留下永久 pending。
#[tokio::test]
async fn shared_context_cancelled_waiter_does_not_strand_gate() {
    let dir = tempfile::tempdir().unwrap();
    let service = SharedContext::new(dir.path().join("app.sqlite"), scope()).unwrap();
    service.snapshot().await.unwrap();
    let (started_tx, mut started_rx) = tokio::sync::mpsc::unbounded_channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let release_rx = Mutex::new(release_rx);
    let worker = service.clone();
    let task = tokio::spawn(async move {
        worker
            .run(Some("owned-write".into()), move |access| {
                started_tx.send(()).unwrap();
                release_rx.lock().unwrap().recv().unwrap();
                access.record_input("t1", ContextOrigin::Voice, "你好")
            })
            .await
    });
    started_rx.recv().await.unwrap();
    task.abort();
    let _ = task.await;
    release_tx.send(()).unwrap();
    assert_eq!(
        service.snapshot().await.unwrap().revision,
        1,
        "后台提交须完成且无重复"
    );
    assert!(
        service
            .execute(ContextOperation::DeleteCard, Some("8"), || Ok(()))
            .is_ok(),
        "取消等待者不能永久关闭 gate"
    );
}

/// 场景：保存失败后切换画布，旧适配器与服务释放，不能丢掉已接受的待保存限制。
/// 构造：临时库注入 checkpoint 更新故障，并提交明确的删除限制。
/// 执行：失败后释放全部外部服务引用，解除故障，在同 scope 新建服务并读取。
/// 预期：宿主恢复原候选，删除 7 仍被拦截；其他目标可执行且只保存一次。
#[tokio::test]
async fn shared_context_pending_survives_releasing_all_consumers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.sqlite");
    let service = SharedContext::new(path.clone(), scope()).unwrap();
    let adapter = adapter::wrap(Arc::new(Checkpoint("succeeded")), service.clone());
    let input = context("不要删除卡片7", "first");
    let frame = adapter.shared_context(&input).await.unwrap().unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_context_checkpoint BEFORE UPDATE ON shared_context_state BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    let result = adapter
        .execute(
            AdapterCommand::UpdateWorkingState(Box::new(ContextStateUpdate {
                input: frame.input,
                expected_revision: frame.snapshot.revision,
                change: ContextStateChange::Constraint {
                    key: "keep".into(),
                    text: input.user_input.clone(),
                    supersedes: None,
                    enforcement: ConstraintEnforcement::Enforced {
                        operation: ContextOperation::DeleteCard,
                        target: Some("7".into()),
                    },
                },
            })),
            &input,
        )
        .await;
    assert!(result.is_err(), "注入保存故障应明确失败");
    drop(adapter);
    drop(service);
    db.execute_batch("DROP TRIGGER fail_context_checkpoint;")
        .unwrap();
    let restored = SharedContext::new(path, scope()).unwrap();
    let snapshot = restored.snapshot().await.unwrap();
    assert_eq!(snapshot.revision, 2, "新消费者应恢复同一宿主接受的候选");
    assert!(
        restored
            .execute(ContextOperation::DeleteCard, Some("7"), || Ok(()))
            .is_err(),
        "切画布不能解除待保存限制"
    );
    assert!(
        restored
            .execute(ContextOperation::DeleteCard, Some("8"), || Ok(()))
            .is_ok(),
        "恢复后不扩大限制范围"
    );
}

struct ScopeChanges(std::sync::atomic::AtomicUsize);
impl AgentCardAdapter for ScopeChanges {
    fn execute<'a>(
        &'a self,
        _: AdapterCommand,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                Ok(AdapterResult::succeeded("current"))
            } else {
                Err(AdapterError::Rejected("scope_changed_during_io".into()))
            }
        })
    }
}

/// 场景：后台读取期间项目/通话已变化，最初 checkpoint 不能代表返回时仍有效。
/// 构造：首个 checkpoint 成功、第二个明确失效的宿主，已有来源记录。
/// 执行：分别请求共享快照和来源补读。
/// 预期：数据返回前再次检查，两条入口都拒绝迟到资料而不是泄漏旧作用域内容。
#[tokio::test]
async fn shared_context_rechecks_source_after_background_read() {
    let dir = tempfile::tempdir().unwrap();
    let service = SharedContext::new(dir.path().join("app.sqlite"), scope()).unwrap();
    let input = service
        .run(None, |access| {
            access.record_input("t1", ContextOrigin::Voice, "你好")
        })
        .await
        .unwrap();
    for read in [false, true] {
        let adapter = adapter::wrap(
            Arc::new(ScopeChanges(std::sync::atomic::AtomicUsize::new(0))),
            service.clone(),
        );
        let result = if read {
            adapter
                .execute(
                    AdapterCommand::ReadSharedContext {
                        reference: input.reference.clone(),
                        offset: 0,
                    },
                    &context("你好", "read"),
                )
                .await
                .map(|_| ())
        } else {
            adapter
                .shared_context(&context("你好", "frame"))
                .await
                .map(|_| ())
        };
        assert!(result.is_err(), "read={read} 必须拒绝读取期间已失效的来源");
    }
}
