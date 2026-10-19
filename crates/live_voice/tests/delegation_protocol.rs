use live_voice::{DelegationNotice, LiveConfig, LiveEvent, LiveSession};

/// AC05：client委派只提供身份和时轴，不把模型额外字段当成用户任务正文。
///
/// 数据构造：同一委派ID重放两次，再以相同ID伪造不同offset；target=responses作负例。
/// 执行过程：ready后收委派、重放和冲突，分别观察公开事件。
/// 预期结果：首个client通知可用、重复忽略、身份冲突拒绝；不采纳task字段或其他backend。
#[test]
fn client_delegation_is_opaque_metadata_and_replays_do_not_dispatch_twice() {
    let mut session = LiveSession::new(LiveConfig::default());
    session.start_message().expect("应启动");
    session
        .receive(
            r#"{"type":"session.started","session":{"id":"live_delegate","model":"gpt-live-1"}}"#,
        )
        .expect("应ready");
    let notice = r#"{"type":"session.delegation.created","offset_ms":1200.5,"delegation":{"id":"d1","type":"delegation","target":"client"},"task":"untrusted-fake-command"}"#;
    let event = session.receive(notice).expect("client委派应解析");
    assert_eq!(
        event,
        LiveEvent::Delegation(DelegationNotice {
            id: "d1".into(),
            offset_ms: 1200.5
        }),
        "只保留官方身份和时轴"
    );
    assert!(
        !format!("{event:?}").contains("untrusted-fake-command"),
        "模型额外task字段不是用户原话"
    );
    assert_eq!(
        session.receive(notice),
        Ok(LiveEvent::Ignored),
        "相同委派不重复发给执行层"
    );
    assert!(session.receive(r#"{"type":"session.delegation.created","offset_ms":9000,"delegation":{"id":"d1","type":"delegation","target":"client"}}"#).is_err(), "同ID改变身份元数据必须拒绝");
    assert_eq!(session.receive(r#"{"type":"session.delegation.created","offset_ms":1300,"delegation":{"id":"d2","type":"delegation","target":"responses"}}"#), Ok(LiveEvent::Ignored), "client模式不执行其他backend委派");
}
