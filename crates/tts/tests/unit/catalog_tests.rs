use crate::builtin_model_catalog;

/// TTS-CATALOG-1：切换到不包含当前 Voice 的模型时必须回退到该模型推荐 Voice。
///
/// 数据构造（含关键数值的推导过程）：
///   catalog models = 2（轻量 Matcha + 多 Voice Kokoro）
///   current voice  = "af_heart"（只属于 Kokoro）
///   Matcha voices  = 1（"default"）
///   fallback count = 1（不兼容 Voice → 推荐 Voice）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从内置目录查找 Matcha 模型；
///   2. 请求保留仅属于 Kokoro 的 "af_heart"；
///   3. 通过目录公共 API 解析切换后的有效 Voice。
///
/// 预期结果：
///   - 正断言：结果为 Matcha 的 "default" 推荐 Voice；
///   - 负断言：结果不得保留不存在的 "af_heart"。
#[test]
fn test_model_switch_falls_back_to_recommended_voice() {
    let catalog = builtin_model_catalog();
    let matcha = catalog
        .find_by_str("matcha-icefall-zh-baker")
        .expect("内置目录应包含轻量 Matcha 中文模型");

    let resolved = matcha.resolve_voice_id(Some("af_heart"));

    assert_eq!(
        resolved.as_str(),
        "default",
        "不属于 Matcha 的 Voice 必须回退到其推荐 default Voice"
    );
    assert_ne!(
        resolved.as_str(),
        "af_heart",
        "切换模型后不得保留新模型不存在的旧 Voice"
    );
}
