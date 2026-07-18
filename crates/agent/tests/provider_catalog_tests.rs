use agent::{normalize_provider_models, ProviderCatalogError};

/// OpenAI-compatible 与 Ollama 模型目录：统一解析、稳定排序，并区分可手动回退的目录错误
///
/// 数据构造（含关键数值的推导过程）：
///   OpenAI data entries = 3（其中 gpt-beta 重复 1 次）→ unique = 2
///   Ollama models entries = 2 → unique = 2
///   empty data entries = 0 → ModelListUnavailable
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 解析 OpenAI `data[].id`，排序并去重
///   2. 解析 Ollama `models[].name`，排序
///   3. 解析空 `data` 列表并检查错误类别
///
/// 预期结果：
///   - 正断言：两种协议均返回稳定排序的精确模型 id
///   - 正断言：目录缺失允许进入手动模型验证
///   - 负断言：重复项不得保留，空目录不得被当作连接成功
#[test]
fn provider_model_lists_are_normalized_for_selection() {
    let openai = serde_json::json!({
        "data": [
            {"id": "gpt-beta"},
            {"id": "gpt-alpha"},
            {"id": "gpt-beta"}
        ]
    });
    let ollama = serde_json::json!({
        "models": [
            {"name": "qwen:latest"},
            {"name": "llama:latest"}
        ]
    });

    let openai_models = normalize_provider_models(&openai)
        .expect("valid OpenAI-compatible model list should parse");
    let ollama_models =
        normalize_provider_models(&ollama).expect("valid Ollama model list should parse");
    let empty = normalize_provider_models(&serde_json::json!({"data": []}));

    assert_eq!(
        openai_models,
        vec!["gpt-alpha".to_string(), "gpt-beta".to_string()],
        "OpenAI-compatible models must be sorted and deduplicated"
    );
    assert_eq!(
        ollama_models,
        vec!["llama:latest".to_string(), "qwen:latest".to_string()],
        "Ollama models must be parsed from name and sorted"
    );
    assert_eq!(
        openai_models.len(),
        2,
        "duplicate OpenAI model ids must not produce duplicate choices"
    );
    assert!(
        matches!(empty, Err(ProviderCatalogError::ModelListUnavailable(_))),
        "empty model list must be classified as a manual-fallback catalog error"
    );
    assert!(
        empty
            .as_ref()
            .is_err_and(ProviderCatalogError::allows_manual_model),
        "model-list-only failures must explicitly allow manual model validation"
    );
    assert_ne!(
        empty,
        Ok(Vec::new()),
        "empty provider model list must never be accepted as a successful connection"
    );
}
