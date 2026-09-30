use canvas::{AgentProfile, AgentSpec, NodeKind};

/// Agent 定义身份应随快照往返，同时兼容旧的 alias-only Agent 卡片。
///
/// 数据构造（含关键数值推导）：
///   bound definition_id = 42，选取固定正整数模拟 SQLite 定义主键
///   legacy_json 字段数 = 2：profile + alias，缺少 definition_id
///   新快照字段数 = 4：profile + alias + definition_id + mobile_control
///
/// 执行过程：
///   1. 构造 definition_id=42 的 AgentSpec 并序列化、反序列化
///   2. 反序列化缺少 definition_id 的旧 JSON
///   3. 分别读取新旧 AgentSpec 的定义身份
///
/// 预期结果：
///   - 正断言：新快照恢复 definition_id=42 与 alias=Alan
///   - 正断言：旧快照继续恢复为 Agent 且 definition_id=None
///   - 负断言：旧快照不能凭 alias 伪造 definition_id=42
#[test]
fn test_agent_definition_id_round_trips_and_legacy_snapshot_defaults_to_unbound() {
    let bound = NodeKind::Agent(AgentSpec {
        profile: AgentProfile::Codex,
        alias: Some("Alan".to_string()),
        definition_id: Some(42),
        mobile_control: false,
    });
    let json = serde_json::to_string(&bound).expect("绑定 Agent 应可序列化到画布快照");
    let restored: NodeKind = serde_json::from_str(&json).expect("绑定 Agent 快照应可反序列化");
    let legacy: NodeKind = serde_json::from_str(r#"{"Agent":{"profile":"Codex","alias":"Alan"}}"#)
        .expect("旧 alias-only Agent 快照应保持兼容");

    let NodeKind::Agent(restored_spec) = restored else {
        panic!("新快照应恢复为 Agent kind");
    };
    let NodeKind::Agent(legacy_spec) = legacy else {
        panic!("旧快照应恢复为 Agent kind");
    };

    assert_eq!(
        restored_spec.definition_id,
        Some(42),
        "新快照必须保留稳定 Agent definition_id=42"
    );
    assert_eq!(
        restored_spec.alias.as_deref(),
        Some("Alan"),
        "definition_id 往返时显示名缓存仍应保留 Alan"
    );
    assert_eq!(
        legacy_spec.definition_id, None,
        "旧快照缺少 definition_id 时必须显式恢复为未绑定状态"
    );
    assert_ne!(
        legacy_spec.definition_id,
        Some(42),
        "旧快照不能仅凭 alias 猜测并伪造 definition_id=42"
    );
}
