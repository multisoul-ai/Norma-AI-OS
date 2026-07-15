use crate::AsrModelId;

/// ASR-ID-1：模型 ID 必须是可持久化的非空不可变身份。
///
/// 数据构造（含关键数值的推导过程）：
///   valid id   = "zipformer-streaming-zh-int8-2025-06-30"，包含精确版本
///   newer id   = "zipformer-streaming-zh-int8-2026-01-01"，版本日期不同
///   empty id   = "   "，trim 后长度 = 0
///   JSON count = 1 个字符串值，不包含路径或模型字节
///
/// 执行过程（逐步说明系统如何处理）：
///   1. parse(valid) → 生成 AsrModelId
///   2. 序列化后再反序列化 → 恢复同一身份
///   3. parse(empty) → 拒绝空身份
///
/// 预期结果：
///   - 正断言：有效 ID 序列化往返后保持不变
///   - 负断言：空白 ID 不能构造成模型身份
#[test]
fn test_model_id_round_trips_and_rejects_blank_values() {
    let model_id = AsrModelId::parse("zipformer-streaming-zh-int8-2025-06-30")
        .expect("带版本的 Zipformer ID 应可构造");
    let json = serde_json::to_string(&model_id).expect("模型 ID 应可序列化");
    let restored: AsrModelId = serde_json::from_str(&json).expect("模型 ID 应可反序列化");
    let newer_version = AsrModelId::parse("zipformer-streaming-zh-int8-2026-01-01")
        .expect("另一个不可变版本 ID 应可构造");

    assert_eq!(
        restored, model_id,
        "序列化往返后必须保留精确的不可变模型 ID"
    );
    assert_eq!(
        model_id.as_str(),
        "zipformer-streaming-zh-int8-2025-06-30",
        "模型 ID 不应在构造时丢失版本信息"
    );
    assert!(
        AsrModelId::parse("   ").is_err(),
        "trim 后为空的 ID 必须被拒绝，否则 Settings 可产生无效选择"
    );
    assert!(
        serde_json::from_str::<AsrModelId>("\"   \"").is_err(),
        "反序列化入口也必须拒绝空白 ID，不能绕过 parse 的领域不变量"
    );
    assert_ne!(
        model_id, newer_version,
        "不同版本日期必须产生不同身份，应用更新不得静默替换模型字节"
    );
}
