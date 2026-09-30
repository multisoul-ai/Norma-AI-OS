use canvas::{AgentSpec, NodeKind};

/// 手机控制模式仅由新快照明确启用，旧卡片不得在恢复时被自动迁移。
///
/// 数据构造：旧 JSON 只有 profile；新 JSON 在同一 profile 上增加 mobile_control=true。
/// 字段缺失表示旧桌面会话，而不是手机控制的默认授权。
///
/// 执行过程：解析两份快照 → 新快照序列化后再次解析 → 检查两个模式。
///
/// 预期结果：新卡片保留受控模式；旧卡片仍为桌面模式；两种模式不相等。
#[test]
fn mobile_mode_round_trips_without_migrating_legacy_cards() {
    let legacy: AgentSpec =
        serde_json::from_str(r#"{"profile":"Codex"}"#).expect("旧卡片必须仍可读取");
    let mobile: NodeKind =
        serde_json::from_str(r#"{"Agent":{"profile":"Codex","mobile_control":true}}"#)
            .expect("手机卡片快照必须可读取");
    let saved = serde_json::to_string(&mobile).expect("手机控制模式必须可持久化");
    let NodeKind::Agent(restored) = serde_json::from_str(&saved).expect("新快照必须可恢复")
    else {
        panic!("恢复后的卡片必须仍为 Agent");
    };
    assert!(restored.mobile_control, "新卡片恢复后不得丢失受控模式");
    assert!(
        !legacy.mobile_control,
        "字段缺失的旧卡片不得自动开启手机控制"
    );
    assert_ne!(
        restored.mobile_control, legacy.mobile_control,
        "新旧模式必须保持区别"
    );
}
