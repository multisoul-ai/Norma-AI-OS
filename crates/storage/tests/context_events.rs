use agent::context::*;
use storage::ContextStore;

/// 场景：最终用户输入与真实操作回执共享，但记录动作不会再次执行。
/// 构造：豆包最终文本及引用它的宿主 accepted / failed 回执，另构造幕僚伪造回执。
/// 执行：通过真实存储接口写入、重复写入，再按一致快照读取。
/// 预期：保留原始来源和失败状态；模型不能冒充宿主回执；读取只返回事实。
#[test]
fn input_and_receipts_preserve_origin_without_replaying_operations() {
    let dir = tempfile::tempdir().unwrap();
    let store = ContextStore::new(dir.path().join("app.sqlite"));
    let scope = ContextScope {
        workspace_id: "w".into(),
        area_id: "a".into(),
        project: ContextProject::Personal,
        task_id: Some("task-1".into()),
        conversation_id: None,
    };
    let input = ContextEvent {
        event_id: "input".into(),
        scope: scope.clone(),
        source: ContextSource {
            origin: ContextOrigin::DoubaoVoice,
            stream_id: "voice-1".into(),
            sequence: 1,
        },
        created_at_ms: 1,
        related: vec![],
        payload: ContextPayload::UserInput {
            text: "启动测试".into(),
        },
    };
    let saved = store.append(&input, None).unwrap();
    let mut receipt = ContextEvent {
        event_id: "accepted".into(),
        scope: scope.clone(),
        source: ContextSource {
            origin: ContextOrigin::Host,
            stream_id: "host-1".into(),
            sequence: 1,
        },
        created_at_ms: 2,
        related: vec![saved.reference],
        payload: ContextPayload::Receipt {
            operation_id: "operation-1".into(),
            status: ReceiptStatus::Accepted,
            summary: "已提交".into(),
        },
    };
    store.append(&receipt, None).unwrap();
    receipt.event_id = "failed".into();
    receipt.source.sequence = 2;
    receipt.payload = ContextPayload::Receipt {
        operation_id: "operation-1".into(),
        status: ReceiptStatus::Failed,
        summary: "启动失败".into(),
    };
    store.append(&receipt, None).unwrap();
    assert_eq!(
        store.append(&receipt, None).unwrap().reference.revision,
        3,
        "重试回执不增加事实或重执行"
    );
    receipt.event_id = "forged".into();
    receipt.source.origin = ContextOrigin::Brain;
    assert!(
        store.append(&receipt, None).is_err(),
        "幕僚不能伪造真实执行回执"
    );
    let snapshot = store.snapshot(&scope, 20).unwrap();
    assert_eq!(
        snapshot.events[0].event.source.origin,
        ContextOrigin::DoubaoVoice,
        "共享后仍保留输入来源"
    );
    assert_eq!(
        snapshot.events.len(),
        3,
        "只能保存一次输入和两个不同阶段的回执"
    );
    assert!(
        matches!(
            snapshot.events[2].event.payload,
            ContextPayload::Receipt {
                status: ReceiptStatus::Failed,
                ..
            }
        ),
        "失败不能被提升为成功"
    );
    assert!(
        snapshot.state.constraints.is_empty(),
        "记录事实不自动改约束或派活"
    );
}
