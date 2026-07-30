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

/// Provider 目录错误分类：只有连接故障可视为临时问题，认证失败必须要求修复凭据。
///
/// 数据构造（含关键数值的推导过程）：
///   error kinds = 3（Connection + Authentication + ModelListUnavailable）
///   temporary kinds = 1（仅 Connection）
///   manual-model kinds = 1（仅 ModelListUnavailable）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别构造临时连接、认证拒绝与模型目录不可用错误。
///   2. 查询每种错误是否临时、是否允许手动输入模型。
///   3. 对认证错误同时检查两种回退均未开放。
///
/// 预期结果：
///   - 正断言：Connection 被标为临时，ModelListUnavailable 允许手动模型。
///   - 负断言：Authentication 既不是临时错误，也不能靠手动模型绕过。
///   - 负断言：Connection 不得被误判为认证失败后的手动回退。
#[test]
fn provider_catalog_error_classification_separates_network_from_authentication() {
    let connection = ProviderCatalogError::Connection("network unavailable".to_string());
    let authentication = ProviderCatalogError::Authentication("credentials rejected".to_string());
    let model_list =
        ProviderCatalogError::ModelListUnavailable("model list unavailable".to_string());

    assert!(
        connection.is_temporary(),
        "Connection must remain a retryable temporary failure"
    );
    assert!(
        model_list.allows_manual_model(),
        "ModelListUnavailable must allow exact manual model entry"
    );
    assert!(
        !authentication.is_temporary(),
        "Authentication must require credential repair instead of preserving ready state"
    );
    assert!(
        !authentication.allows_manual_model(),
        "Authentication must not be bypassed through manual model entry"
    );
    assert!(
        !connection.allows_manual_model(),
        "Connection must not be mistaken for a model-list-only failure"
    );
    assert!(
        !model_list.is_temporary(),
        "ModelListUnavailable must not inherit the completed-configuration network exception"
    );
}
