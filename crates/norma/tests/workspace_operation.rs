use norma::workspace_operation::{WorkspaceOperationOrigin, WorkspaceOperationSource};

/// 桌面来源与手机来源由不同typed字段表示，不能共享伪造device主体。
/// 数据构造：相同host/device字符串和相同operation，不同origin variant。
/// 执行过程：序列化来源键并尝试desktop携带mobile字段。
/// 预期结果：来源键不同，未知字段拒绝，来源验证不需要mobile token。
#[test]
fn desktop_and_mobile_origins_are_distinct_and_reject_unknown_fields() {
    let desktop = WorkspaceOperationOrigin::Desktop {
        host_id: "same-id".into(),
    };
    let mobile = WorkspaceOperationOrigin::Mobile {
        device_id: "same-id".into(),
    };
    assert_ne!(desktop.key(), mobile.key(), "两个入口不能共享持久幂等键");
    assert!(
        serde_json::from_str::<WorkspaceOperationOrigin>(
            r#"{"kind":"desktop","host_id":"same-id","device_id":"forged"}"#
        )
        .is_err(),
        "desktop必须拒绝伪手机字段"
    );
    let source = WorkspaceOperationSource {
        workspace_id: "workspace-a".into(),
        origin: desktop,
        operation_id: "operation-a".into(),
        message_id: "message-a".into(),
        original_text: "让 Alan 工作".into(),
    };
    assert!(
        source.validate().is_ok(),
        "桌面可信来源不需要伪 token/device"
    );
}
