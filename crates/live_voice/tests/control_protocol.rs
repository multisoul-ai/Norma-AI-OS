use live_voice::{
    Acknowledgement, AppendKind, LiveCommand, LiveConfig, LiveError, LiveEvent, LiveSession,
};
use serde_json::Value;

/// AC10：结果按原委派ID回传，ACK只说明命令已接受，不能证明任务完成或播完。
///
/// 数据构造：d1是已知委派；unknown不是。501字节中文超过500字节保守内容上限。
/// 执行过程：ready前发送、用未知ID发送、注册d1后发送，再接收ACK并静音。
/// 预期结果：只有有效关联通过；通用上下文显式null；不改变音频用量的最终确认。
#[test]
fn context_updates_keep_task_identity_and_acknowledgements_are_not_delivery() {
    let mut session = LiveSession::new(LiveConfig::default());
    let mut command = LiveCommand::Append {
        event_id: "result_1".into(),
        kind: AppendKind::Commentary,
        delegation_id: Some("d1".into()),
        content: "任务已接收，还未完成".into(),
    };
    assert_eq!(
        session.command_message(&command),
        Err(LiveError::NotReady),
        "ready前不能回传结果"
    );
    session.start_message().expect("应启动");
    session
        .receive(
            r#"{"type":"session.started","session":{"id":"live_updates","model":"gpt-live-1"}}"#,
        )
        .expect("应ready");
    assert_eq!(
        session.command_message(&command),
        Err(LiveError::InvalidCommand),
        "不能用未知委派ID回传"
    );
    session.receive(r#"{"type":"session.delegation.created","offset_ms":1000,"delegation":{"id":"d1","type":"delegation","target":"client"}}"#).expect("已知委派");
    let body: Value =
        serde_json::from_str(&session.command_message(&command).expect("有效结果应可发送"))
            .expect("结果应为JSON");
    assert_eq!(
        body["type"], "session.commentary.append",
        "已验证事实通过commentary回传"
    );
    assert_eq!(body["delegation_id"], "d1", "保留原始Live委派ID");
    assert_eq!(
        body["content"], "任务已接收，还未完成",
        "状态描述不得提升为任务完成"
    );
    let ack = session
        .receive(r#"{"type":"session.commentary.appended","client_event_id":"result_1"}"#)
        .expect("ACK有效");
    assert_eq!(
        ack,
        LiveEvent::Acknowledged {
            client_event_id: "result_1".into(),
            kind: Acknowledgement::Appended(AppendKind::Commentary)
        },
        "ACK保留命令关联且不是播放终态"
    );
    assert!(
        !session.usage().finalized,
        "ACK不能冒充session.closed或播放确认"
    );
    command = LiveCommand::Append {
        event_id: "general".into(),
        kind: AppendKind::Thinking,
        delegation_id: None,
        content: "当前未打开项目".into(),
    };
    let general: Value =
        serde_json::from_str(&session.command_message(&command).expect("允许通用上下文"))
            .expect("通用消息有效");
    assert!(
        general.get("delegation_id").is_some_and(Value::is_null),
        "通用上下文必须显式携带null"
    );
    command = LiveCommand::Append {
        event_id: "too_long".into(),
        kind: AppendKind::Thinking,
        delegation_id: None,
        content: "中".repeat(167),
    };
    assert_eq!(
        session.command_message(&command),
        Err(LiveError::InvalidCommand),
        "内容必须切成保守不超500tokens的块"
    );
    let mute: Value = serde_json::from_str(
        &session
            .command_message(&LiveCommand::Mute {
                event_id: "mute_1".into(),
                muted: true,
            })
            .expect("允许静音输入"),
    )
    .expect("静音JSON有效");
    assert_eq!(
        mute["type"], "session.input_audio.mute",
        "静音只控制输入，不能发送取消任务"
    );
}

/// AC10/AC11：官方ACK的client_event_id可省略；无法关联时忽略，不能断开合法会话。
///
/// 数据构造：ready会话依次收到mute/unmute及三类append的无关联ID确认。
/// 执行过程：通过公开协议入口处理五种ACK，然后再次构造控制命令。
/// 预期结果：每种无关联ACK均Ignored，连接状态仍允许发送命令。
#[test]
fn acknowledgement_without_optional_correlation_does_not_reject_the_session() {
    let mut session = LiveSession::new(LiveConfig::default());
    session.start_message().expect("启动");
    session.receive(r#"{"type":"session.started","session":{"id":"live_optional_ack","model":"gpt-live-1"}}"#).expect("ready");
    for kind in [
        "session.input_audio.muted",
        "session.input_audio.unmuted",
        "session.instructions.appended",
        "session.thinking.appended",
        "session.commentary.appended",
    ] {
        let event = serde_json::json!({"type":kind});
        assert_eq!(
            session.receive(&event.to_string()),
            Ok(LiveEvent::Ignored),
            "合法{kind}无关联ID时不能切断会话"
        );
    }
    assert!(
        session
            .command_message(&LiveCommand::Mute {
                event_id: "next".into(),
                muted: true
            })
            .is_ok(),
        "无关联ACK后会话仍应可用"
    );
}
